use std::sync::Arc;
use tracing::{debug, info, info_span, Instrument};

use uc_core::{
    ports::{
        security::{
            encryption_state::EncryptionStatePort,
            key_scope::{KeyScopePort, ScopeError},
        },
        EncryptionPort, EncryptionSessionPort, KeyMaterialPort,
    },
    security::{
        model::{
            EncryptionAlgo, EncryptionError, KeySlot, MasterKey, Passphrase, WrappedMasterKey,
        },
        state::{EncryptionState, EncryptionStateError},
    },
};

#[derive(Debug, thiserror::Error)]
pub enum InitializeEncryptionError {
    #[error("encryption is already initialized")]
    AlreadyInitialized,

    #[error("failed to encrypt master key")]
    EncryptionFailed(#[from] EncryptionError),

    #[error("failed to persist encryption state")]
    StatePersistenceFailed(#[from] EncryptionStateError),

    #[error("failed to resolve key scope")]
    ScopeFailed(#[from] ScopeError),
}

/// Use case for initializing encryption with a passphrase.
///
/// ## Architecture / 架构
///
/// This use case uses **trait objects** (`dyn Port`) instead of generic type parameters.
/// This is the recommended pattern for use cases in the uc-app layer:
///
/// - **Type stability**: The use case has a concrete type, not a generic one
/// - **Easy testing**: Can easily mock ports in tests
/// - **Bootstrap simplicity**: UseCases accessor can instantiate this with Arc<dyn Port>
///
/// 此用例使用 **trait 对象** (`dyn Port`) 而不是泛型类型参数。
/// 这是 uc-app 层用例的推荐模式：
///
/// - **类型稳定性**：用例具有具体类型，而不是泛型类型
/// - **易于测试**：可以轻松在测试中模拟端口
/// - **装配简单性**：UseCases 访问器可以用 Arc<dyn Port> 实例化此用例
///
/// ## Trade-offs / 权衡
///
/// - **Pros**: Clean separation, type stability, easier DI
/// - **Cons**: Slight runtime overhead from dynamic dispatch (negligible for I/O-bound operations)
///
/// ## 优势**：清晰的分离、类型稳定性、更容易的依赖注入
/// ## **劣势**：动态分发带来的轻微运行时开销（对于 I/O 密集型操作可忽略不计）
pub struct InitializeEncryption {
    encryption: Arc<dyn EncryptionPort>,
    key_material: Arc<dyn KeyMaterialPort>,
    key_scope: Arc<dyn KeyScopePort>,
    encryption_state_repo: Arc<dyn EncryptionStatePort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
}

impl InitializeEncryption {
    /// Create a new InitializeEncryption use case from trait objects.
    /// 从 trait 对象创建新的 InitializeEncryption 用例。
    ///
    /// This follows the `dyn Port` pattern recommended for uc-app use cases.
    /// 遵循 uc-app 用例推荐的 `dyn Port` 模式。
    pub fn new(
        encryption: Arc<dyn EncryptionPort>,
        key_material: Arc<dyn KeyMaterialPort>,
        key_scope: Arc<dyn KeyScopePort>,
        encryption_state_repo: Arc<dyn EncryptionStatePort>,
        encryption_session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self {
            encryption,
            key_material,
            key_scope,
            encryption_state_repo,
            encryption_session,
        }
    }

    /// Create a new InitializeEncryption use case from cloned Arc<dyn Port> references.
    /// 从克隆的 Arc<dyn Port> 引用创建新的 InitializeEncryption 用例。
    ///
    /// This is a convenience method for the UseCases accessor pattern.
    /// 这是 UseCases 访问器模式的便捷方法。
    pub fn from_ports(
        encryption: Arc<dyn EncryptionPort>,
        key_material: Arc<dyn KeyMaterialPort>,
        key_scope: Arc<dyn KeyScopePort>,
        encryption_state_repo: Arc<dyn EncryptionStatePort>,
        encryption_session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self::new(
            encryption,
            key_material,
            key_scope,
            encryption_state_repo,
            encryption_session,
        )
    }

    pub async fn execute(&self, passphrase: Passphrase) -> Result<(), InitializeEncryptionError> {
        let span = info_span!("usecase.initialize_encryption.execute");

        async {
            info!("Starting encryption initialization");

            let state = self.encryption_state_repo.load_state().await?;
            debug!(state = ?state, "Loaded encryption state");

            // 1. assert not initialized
            if state == EncryptionState::Initialized {
                return Err(InitializeEncryptionError::AlreadyInitialized);
            }

            debug!("Getting current scope");
            let scope = self.key_scope.current_scope().await?;
            debug!(scope = %scope.to_identifier(), "Got scope");

            debug!("Creating keyslot draft");
            let keyslot_draft = KeySlot::draft_v1(scope.clone())?;
            debug!("Keyslot draft created");

            // 2. derive KEK
            debug!("Deriving KEK");
            let kek = self
                .encryption
                .derive_kek(&passphrase, &keyslot_draft.salt, &keyslot_draft.kdf)
                .await?;
            debug!("KEK derived successfully");

            // 3. generate MasterKey
            debug!("Generating master key");
            let master_key = MasterKey::generate()?;
            debug!("Master key generated");

            // 4. wrap MasterKey
            debug!("Wrapping master key");
            let blob = self
                .encryption
                .wrap_master_key(&kek, &master_key, EncryptionAlgo::XChaCha20Poly1305)
                .await?;
            debug!("Master key wrapped successfully");

            let keyslot = keyslot_draft.finalize(WrappedMasterKey { blob });
            debug!("Keyslot finalized");

            // 5. persist wrapped key, store keyslot
            debug!("Storing keyslot");
            self.key_material.store_keyslot(&keyslot).await?;
            debug!("Keyslot stored successfully");

            // 6. store KEK material into keyring
            debug!("Storing KEK in keyring");
            self.key_material.store_kek(&scope, &kek).await?;
            debug!("KEK stored successfully");

            // 7. persist initialized state
            debug!("Persisting initialized state");
            self.encryption_state_repo.persist_initialized().await?;
            debug!("Encryption state persisted");

            // 8. set master key in session for immediate use
            debug!("Setting master key in session");
            self.encryption_session.set_master_key(master_key).await?;
            debug!("Master key set in session successfully");

            info!("Encryption initialized successfully");
            Ok(())
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
    use uc_core::security::model::{
        EncryptedBlob, EncryptionAlgo, EncryptionFormatVersion, Kek, KeyScope,
    };

    #[tokio::test]
    async fn test_initialize_encryption_sets_master_key_in_session() {
        // Test that initialization sets the master key in the session
        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Uninitialized));
        state.expect_persist_initialized().returning(|| Ok(()));

        let scope_value = KeyScope {
            profile_id: "test".to_string(),
        };
        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(move || Ok(scope_value.clone()));

        let mut key_material = MockKeyMaterial::new();
        key_material.expect_store_keyslot().returning(|_| Ok(()));
        key_material.expect_store_kek().returning(|_, _| Ok(()));

        let mut encryption = MockEncryption::new();
        encryption
            .expect_derive_kek()
            .returning(|_, _, _| Ok(Kek([0u8; 32])));
        encryption.expect_wrap_master_key().returning(|_, _, _| {
            Ok(EncryptedBlob {
                version: EncryptionFormatVersion::V1,
                aead: EncryptionAlgo::XChaCha20Poly1305,
                nonce: vec![0u8; 24],
                ciphertext: vec![0u8; 32],
                aad_fingerprint: None,
            })
        });

        // Expect set_master_key to be called exactly once (verifies master key was set)
        let mut session = MockEncryptionSession::new();
        session
            .expect_set_master_key()
            .times(1)
            .returning(|_| Ok(()));

        let use_case = InitializeEncryption::new(
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(scope),
            Arc::new(state),
            Arc::new(session),
        );

        let passphrase = Passphrase("test-password".to_string());
        let result = use_case.execute(passphrase).await;

        assert!(result.is_ok(), "initialization should succeed");
    }

    #[tokio::test]
    async fn test_initialize_encryption_fails_when_already_initialized() {
        // Test that initialization fails when already initialized
        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let scope = MockKeyScope::new();
        let key_material = MockKeyMaterial::new();
        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();

        let use_case = InitializeEncryption::new(
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(scope),
            Arc::new(state),
            Arc::new(session),
        );

        let passphrase = Passphrase("test-password".to_string());
        let result = use_case.execute(passphrase).await;

        assert!(result.is_err(), "initialization should fail");
        let err = result.unwrap_err();
        assert!(matches!(err, InitializeEncryptionError::AlreadyInitialized));
    }

    #[tokio::test]
    async fn test_initialize_encryption_does_not_set_session_on_failure() {
        // Test that session is not set when initialization fails (AlreadyInitialized path)
        // mockall verifies set_master_key is NOT called since no expectation is set for it
        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let scope = MockKeyScope::new();
        let key_material = MockKeyMaterial::new();
        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new(); // no expect_set_master_key = must not be called

        let use_case = InitializeEncryption::new(
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(scope),
            Arc::new(state),
            Arc::new(session),
        );

        let passphrase = Passphrase("test-password".to_string());
        let _ = use_case.execute(passphrase).await;
        // mockall will panic on drop if set_master_key was unexpectedly called
    }

    #[tokio::test]
    async fn test_initialize_encryption_stores_kek_and_keyslot() {
        // Test that both kek and keyslot are stored during initialization
        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Uninitialized));
        state.expect_persist_initialized().returning(|| Ok(()));

        let scope_value = KeyScope {
            profile_id: "test".to_string(),
        };
        let mut scope = MockKeyScope::new();
        scope
            .expect_current_scope()
            .returning(move || Ok(scope_value.clone()));

        let mut key_material = MockKeyMaterial::new();
        // Expect exactly 1 call to each store method
        key_material
            .expect_store_keyslot()
            .times(1)
            .returning(|_| Ok(()));
        key_material
            .expect_store_kek()
            .times(1)
            .returning(|_, _| Ok(()));

        let mut encryption = MockEncryption::new();
        encryption
            .expect_derive_kek()
            .returning(|_, _, _| Ok(Kek([0u8; 32])));
        encryption.expect_wrap_master_key().returning(|_, _, _| {
            Ok(EncryptedBlob {
                version: EncryptionFormatVersion::V1,
                aead: EncryptionAlgo::XChaCha20Poly1305,
                nonce: vec![0u8; 24],
                ciphertext: vec![0u8; 32],
                aad_fingerprint: None,
            })
        });

        let mut session = MockEncryptionSession::new();
        session.expect_set_master_key().returning(|_| Ok(()));

        let use_case = InitializeEncryption::new(
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(scope),
            Arc::new(state),
            Arc::new(session),
        );

        let passphrase = Passphrase("test-password".to_string());
        let result = use_case.execute(passphrase).await;

        assert!(result.is_ok(), "initialization should succeed");
        // mockall verifies store_keyslot and store_kek were each called exactly once on drop
    }

    #[tokio::test]
    async fn test_initialize_encryption_does_not_store_keys_on_failure() {
        // Test that keys are not stored when initialization fails (AlreadyInitialized path)
        // mockall verifies store_kek and store_keyslot are NOT called since no expectations are set
        let mut state = MockEncryptionState::new();
        state
            .expect_load_state()
            .returning(|| Ok(EncryptionState::Initialized));

        let scope = MockKeyScope::new();
        let key_material = MockKeyMaterial::new(); // no store_* expectations = must not be called
        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();

        let use_case = InitializeEncryption::new(
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(scope),
            Arc::new(state),
            Arc::new(session),
        );

        let passphrase = Passphrase("test-password".to_string());
        let _ = use_case.execute(passphrase).await;
        // mockall will panic on drop if store_kek or store_keyslot were unexpectedly called
    }
}
