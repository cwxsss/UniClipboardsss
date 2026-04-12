//! Auto-unlock encryption session on startup.
//!
//! This use case loads the MasterKey from persisted keyslot + KEK
//! and sets it in the EncryptionSessionPort for transparent encryption.

use std::sync::Arc;
use tracing::{info, info_span, Instrument};

use uc_core::{
    ports::{
        security::{encryption_state::EncryptionStatePort, key_scope::KeyScopePort},
        EncryptionPort, EncryptionSessionPort, KeyMaterialPort,
    },
    security::{model::EncryptionError, state::EncryptionState},
};

#[derive(Debug, thiserror::Error)]
pub enum AutoUnlockError {
    #[error("encryption state check failed: {0}")]
    StateCheckFailed(String),

    #[error("key scope resolution failed: {0}")]
    ScopeFailed(String),

    #[error("failed to load keyslot: {0}")]
    KeySlotLoadFailed(#[source] EncryptionError),

    #[error("failed to load KEK from keyring: {0}")]
    KekLoadFailed(#[source] EncryptionError),

    #[error("keyslot has no wrapped master key")]
    MissingWrappedMasterKey,

    #[error("failed to unwrap master key: {0}")]
    UnwrapFailed(#[source] EncryptionError),

    #[error("failed to set master key in session: {0}")]
    SessionSetFailed(#[source] EncryptionError),
}

/// Use case for automatically unlocking encryption session on startup.
///
/// ## Behavior
///
/// - If encryption is **Uninitialized**: Returns `Ok(false)` (not unlocked, but not an error)
/// - If encryption is **Initialized**: Attempts to load and set MasterKey, returns `Ok(true)` on success
/// - Any failure during unlock returns an error
pub struct AutoUnlockEncryptionSession {
    encryption_state: Arc<dyn EncryptionStatePort>,
    key_scope: Arc<dyn KeyScopePort>,
    key_material: Arc<dyn KeyMaterialPort>,
    encryption: Arc<dyn EncryptionPort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
}

impl AutoUnlockEncryptionSession {
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

    /// Execute the keyring unlock flow.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Session unlocked successfully
    /// - `Ok(false)` - Encryption not initialized (no unlock needed)
    /// - `Err(_)` - Unlock failed
    pub async fn execute(&self) -> Result<bool, AutoUnlockError> {
        let span = info_span!("usecase.auto_unlock_encryption_session.execute");

        async {
            info!("Checking encryption state for keyring unlock");

            // 1. Check encryption state
            let state = self
                .encryption_state
                .load_state()
                .await
                .map_err(|e| AutoUnlockError::StateCheckFailed(e.to_string()))?;

            if state == EncryptionState::Uninitialized {
                info!("Encryption not initialized, skipping keyring unlock");
                return Ok(false);
            }

            info!("Encryption initialized, attempting keyring unlock");

            // 2. Get key scope
            let scope = self
                .key_scope
                .current_scope()
                .await
                .map_err(|e| AutoUnlockError::ScopeFailed(e.to_string()))?;

            // 3. Load keyslot
            let keyslot = self
                .key_material
                .load_keyslot(&scope)
                .await
                .map_err(AutoUnlockError::KeySlotLoadFailed)?;

            // 4. Get wrapped master key
            let wrapped_master_key = keyslot
                .wrapped_master_key
                .ok_or(AutoUnlockError::MissingWrappedMasterKey)?;

            // 5. Load KEK from keyring
            let kek = self
                .key_material
                .load_kek(&scope)
                .await
                .map_err(AutoUnlockError::KekLoadFailed)?;

            // 6. Unwrap master key
            let master_key = self
                .encryption
                .unwrap_master_key(&kek, &wrapped_master_key.blob)
                .await
                .map_err(AutoUnlockError::UnwrapFailed)?;

            // 7. Set master key in session
            self.encryption_session
                .set_master_key(master_key)
                .await
                .map_err(AutoUnlockError::SessionSetFailed)?;

            info!("Keyring unlock completed successfully");
            Ok(true)
        }
        .instrument(span)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{
        MockEncryption, MockEncryptionSession, MockEncryptionState, MockKeyMaterial, MockKeyScope,
    };
    use std::sync::Arc;
    use uc_core::security::{
        model::{
            EncryptedBlob, EncryptionAlgo, EncryptionFormatVersion, Kek, KeyScope, MasterKey,
            WrappedMasterKey,
        },
        state::EncryptionStateError,
    };

    /// Creates a valid test keyslot with wrapped master key
    fn create_test_keyslot(scope: KeyScope) -> uc_core::security::model::KeySlot {
        uc_core::security::model::KeySlot {
            version: uc_core::security::model::KeySlotVersion::V1,
            scope,
            kdf: uc_core::security::model::KdfParams::for_initialization(),
            salt: vec![0u8; 16],
            wrapped_master_key: Some(WrappedMasterKey {
                blob: EncryptedBlob {
                    version: EncryptionFormatVersion::V1,
                    aead: EncryptionAlgo::XChaCha20Poly1305,
                    nonce: vec![0u8; 24],
                    ciphertext: vec![0u8; 32],
                    aad_fingerprint: None,
                },
            }),
        }
    }

    /// Creates a test KEK
    fn create_test_kek() -> Kek {
        Kek([0u8; 32])
    }

    #[tokio::test]
    async fn test_auto_unlock_returns_false_when_uninitialized() {
        // When encryption state is Uninitialized, auto-unlock should return Ok(false)
        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Uninitialized));

        let scope = MockKeyScope::new();
        let key_material = MockKeyMaterial::new();
        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(result.is_ok(), "should succeed when uninitialized");
        assert_eq!(
            result.unwrap(),
            false,
            "should return false when uninitialized"
        );
    }

    #[tokio::test]
    async fn test_auto_unlock_succeeds_when_initialized() {
        // When all dependencies succeed, auto-unlock should return Ok(true)
        let scope_value = KeyScope {
            profile_id: "test".to_string(),
        };
        let test_keyslot = create_test_keyslot(scope_value.clone());
        let test_kek = create_test_kek();

        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let scope_clone = scope_value.clone();
        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(move || Ok(scope_clone.clone()));

        let mut key_material = MockKeyMaterial::new();
        key_material
            .expect_load_keyslot()
            .returning(move |_| Ok(test_keyslot.clone()));
        key_material
            .expect_load_kek()
            .returning(move |_| Ok(test_kek.clone()));

        let mut encryption = MockEncryption::new();
        encryption
            .expect_unwrap_master_key()
            .returning(|_, _| MasterKey::from_bytes(&[0u8; 32]));

        let mut session = MockEncryptionSession::new();
        // Expect set_master_key to be called exactly once (verifies master key was set)
        session
            .expect_set_master_key()
            .times(1)
            .returning(|_| Ok(()));

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(
            result.is_ok(),
            "should succeed when all dependencies succeed"
        );
        assert_eq!(
            result.unwrap(),
            true,
            "should return true on successful unlock"
        );
    }

    #[tokio::test]
    async fn test_auto_unlock_propagates_state_check_error() {
        // When state check fails, should return StateCheckFailed error
        let mut state = MockEncryptionState::new();
        state.expect_load_state().returning(|| {
            Err(EncryptionStateError::LoadError(
                "state check failed".to_string(),
            ))
        });

        let scope = MockKeyScope::new();
        let key_material = MockKeyMaterial::new();
        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(result.is_err(), "should fail when state check fails");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("encryption state check failed"),
            "error should indicate state check failure: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_auto_unlock_propagates_scope_error() {
        // When scope resolution fails, should return ScopeFailed error
        use uc_core::ports::security::key_scope::ScopeError;

        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(|| Err(ScopeError::FailedToGetCurrentScope));

        let key_material = MockKeyMaterial::new();
        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(result.is_err(), "should fail when scope resolution fails");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("key scope resolution failed"),
            "error should indicate scope failure: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_auto_unlock_propagates_keyslot_load_error() {
        // When keyslot load fails, should return KeySlotLoadFailed error
        let scope_value = KeyScope {
            profile_id: "test".to_string(),
        };

        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(move || Ok(scope_value.clone()));

        let mut key_material = MockKeyMaterial::new();
        // No keyslot = fails
        key_material
            .expect_load_keyslot()
            .returning(|_| Err(EncryptionError::KeyNotFound));

        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(result.is_err(), "should fail when keyslot load fails");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to load keyslot"),
            "error should indicate keyslot load failure: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_auto_unlock_fails_when_missing_wrapped_master_key() {
        // When keyslot exists but has no wrapped master key, should return MissingWrappedMasterKey
        let scope_value = KeyScope {
            profile_id: "test".to_string(),
        };
        let mut keyslot = create_test_keyslot(KeyScope {
            profile_id: "test".to_string(),
        });
        keyslot.wrapped_master_key = None; // Remove wrapped master key

        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(move || Ok(scope_value.clone()));

        let mut key_material = MockKeyMaterial::new();
        key_material
            .expect_load_keyslot()
            .returning(move |_| Ok(keyslot.clone()));

        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(
            result.is_err(),
            "should fail when wrapped master key is missing"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("keyslot has no wrapped master key"),
            "error should indicate missing wrapped master key: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_auto_unlock_propagates_unwrap_error() {
        // When unwrap fails, should return UnwrapFailed error
        let scope_value = KeyScope {
            profile_id: "test".to_string(),
        };
        let test_keyslot = create_test_keyslot(KeyScope {
            profile_id: "test".to_string(),
        });
        let test_kek = create_test_kek();

        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(move || Ok(scope_value.clone()));

        let mut key_material = MockKeyMaterial::new();
        key_material
            .expect_load_keyslot()
            .returning(move |_| Ok(test_keyslot.clone()));
        key_material
            .expect_load_kek()
            .returning(move |_| Ok(test_kek.clone()));

        let mut encryption = MockEncryption::new();
        encryption
            .expect_unwrap_master_key()
            .returning(|_, _| Err(EncryptionError::CryptoFailure));

        let session = MockEncryptionSession::new();

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(result.is_err(), "should fail when unwrap fails");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to unwrap master key"),
            "error should indicate unwrap failure: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_auto_unlock_propagates_session_set_error() {
        // When session set fails, should return SessionSetFailed error
        let scope_value = KeyScope {
            profile_id: "test".to_string(),
        };
        let test_keyslot = create_test_keyslot(KeyScope {
            profile_id: "test".to_string(),
        });
        let test_kek = create_test_kek();

        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(move || Ok(scope_value.clone()));

        let mut key_material = MockKeyMaterial::new();
        key_material
            .expect_load_keyslot()
            .returning(move |_| Ok(test_keyslot.clone()));
        key_material
            .expect_load_kek()
            .returning(move |_| Ok(test_kek.clone()));

        let mut encryption = MockEncryption::new();
        encryption
            .expect_unwrap_master_key()
            .returning(|_, _| MasterKey::from_bytes(&[0u8; 32]));

        let mut session = MockEncryptionSession::new();
        session
            .expect_set_master_key()
            .returning(|_| Err(EncryptionError::CryptoFailure));

        let use_case = AutoUnlockEncryptionSession::new(
            Arc::new(state),
            Arc::new(scope),
            Arc::new(key_material),
            Arc::new(encryption),
            Arc::new(session),
        );

        let result = use_case.execute().await;

        assert!(result.is_err(), "should fail when session set fails");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to set master key in session"),
            "error should indicate session set failure: {}",
            err
        );
    }
}
