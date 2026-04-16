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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{
        MockEncryption, MockEncryptionSession, MockEncryptionState, MockKeyMaterial, MockKeyScope,
    };
    use std::sync::{Arc, Mutex};
    use uc_core::security::model::{
        EncryptedBlob, EncryptionAlgo, EncryptionError, EncryptionFormatVersion, Kek, KeyScope,
    };

    fn make_encryption_ok(unwrapped_master_key: Option<MasterKey>) -> MockEncryption {
        let mut enc = MockEncryption::new();
        enc.expect_derive_kek()
            .returning(|_, _, _| Ok(Kek([3u8; 32])));
        enc.expect_wrap_master_key().returning(|_, _, _| {
            Ok(EncryptedBlob {
                version: EncryptionFormatVersion::V1,
                aead: EncryptionAlgo::XChaCha20Poly1305,
                nonce: vec![0u8; 24],
                ciphertext: vec![1u8; 32],
                aad_fingerprint: None,
            })
        });
        if let Some(mk) = unwrapped_master_key {
            enc.expect_unwrap_master_key()
                .returning(move |_, _| Ok(mk.clone()));
        } else {
            enc.expect_unwrap_master_key()
                .returning(|_, _| Err(EncryptionError::KeyMaterialCorrupt));
        }
        enc
    }

    fn make_key_scope() -> MockKeyScope {
        let mut ks = MockKeyScope::new();
        ks.expect_current_scope().returning(|| {
            Ok(KeyScope {
                profile_id: "profile-test".to_string(),
            })
        });
        ks
    }

    /// Track calls to store/delete methods on a key material mock.
    struct KeyMaterialTracker {
        store_kek_called: bool,
        store_keyslot_called: bool,
        delete_kek_called: bool,
        delete_keyslot_called: bool,
    }

    fn make_key_material(
        store_kek_error: Option<EncryptionError>,
        store_keyslot_error: Option<EncryptionError>,
    ) -> (MockKeyMaterial, Arc<Mutex<KeyMaterialTracker>>) {
        let tracker = Arc::new(Mutex::new(KeyMaterialTracker {
            store_kek_called: false,
            store_keyslot_called: false,
            delete_kek_called: false,
            delete_keyslot_called: false,
        }));

        let mut km = MockKeyMaterial::new();

        // load_kek / load_keyslot — always fail (not used in these tests)
        km.expect_load_kek()
            .returning(|_| Err(EncryptionError::KeyNotFound));
        km.expect_load_keyslot()
            .returning(|_| Err(EncryptionError::KeyNotFound));

        let t = tracker.clone();
        let err = Arc::new(Mutex::new(store_kek_error));
        km.expect_store_kek().returning(move |_, _| {
            t.lock().unwrap().store_kek_called = true;
            match err.lock().unwrap().take() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        });

        let t = tracker.clone();
        let err = Arc::new(Mutex::new(store_keyslot_error));
        km.expect_store_keyslot().returning(move |_| {
            t.lock().unwrap().store_keyslot_called = true;
            match err.lock().unwrap().take() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        });

        let t = tracker.clone();
        km.expect_delete_kek().returning(move |_| {
            t.lock().unwrap().delete_kek_called = true;
            Ok(())
        });

        let t = tracker.clone();
        km.expect_delete_keyslot().returning(move |_| {
            t.lock().unwrap().delete_keyslot_called = true;
            Ok(())
        });

        (km, tracker)
    }

    /// Track calls to persist_initialized / clear_initialized on encryption state mock.
    struct EncryptionStateTracker {
        persist_initialized_called: bool,
    }

    fn make_encryption_state(
        persist_error: Option<EncryptionStateError>,
    ) -> (MockEncryptionState, Arc<Mutex<EncryptionStateTracker>>) {
        let tracker = Arc::new(Mutex::new(EncryptionStateTracker {
            persist_initialized_called: false,
        }));

        let mut es = MockEncryptionState::new();
        es.expect_load_state()
            .returning(|| Ok(uc_core::security::state::EncryptionState::Uninitialized));
        es.expect_clear_initialized().returning(|| Ok(()));

        let t = tracker.clone();
        let err = Arc::new(Mutex::new(persist_error));
        es.expect_persist_initialized().returning(move || {
            t.lock().unwrap().persist_initialized_called = true;
            match err.lock().unwrap().take() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        });

        (es, tracker)
    }

    /// Track calls to set_master_key / clear on encryption session mock.
    struct EncryptionSessionTracker {
        set_master_key_called: bool,
        clear_called: bool,
    }

    fn make_encryption_session(
        set_master_key_error: Option<EncryptionError>,
    ) -> (MockEncryptionSession, Arc<Mutex<EncryptionSessionTracker>>) {
        let tracker = Arc::new(Mutex::new(EncryptionSessionTracker {
            set_master_key_called: false,
            clear_called: false,
        }));

        let mut sess = MockEncryptionSession::new();
        sess.expect_is_ready().returning(|| false);
        sess.expect_get_master_key()
            .returning(|| Err(EncryptionError::KeyNotFound));

        let t = tracker.clone();
        let err = Arc::new(Mutex::new(set_master_key_error));
        sess.expect_set_master_key().returning(move |_| {
            t.lock().unwrap().set_master_key_called = true;
            match err.lock().unwrap().take() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        });

        let t = tracker.clone();
        sess.expect_clear().returning(move || {
            t.lock().unwrap().clear_called = true;
            Ok(())
        });

        (sess, tracker)
    }

    #[tokio::test]
    async fn space_access_keychain_rollback_on_keyslot_failure() {
        let encryption = make_encryption_ok(None);
        let (key_material, km_tracker) = make_key_material(None, Some(EncryptionError::IoFailure));
        let (encryption_state, _) = make_encryption_state(None);
        let (encryption_session, _) = make_encryption_session(None);

        let adapter = SpaceAccessCryptoAdapter::new(
            SecretString::from("passphrase"),
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(make_key_scope()),
            Arc::new(encryption_state),
            Arc::new(encryption_session),
        );

        let result = adapter.export_keyslot_blob(&SpaceId::new()).await;

        assert!(result.is_err());
        let guard = km_tracker.lock().unwrap();
        assert!(guard.delete_kek_called, "expected KEK rollback");
        assert!(guard.delete_keyslot_called, "expected keyslot cleanup");
    }

    #[tokio::test]
    async fn derive_master_key_from_keyslot_succeeds_and_persists_state() {
        let expected_master_key = MasterKey::from_bytes(&[9u8; 32]).expect("valid master key");
        let encryption = make_encryption_ok(Some(expected_master_key.clone()));
        let (key_material, km_tracker) = make_key_material(None, None);
        let (encryption_state, state_tracker) = make_encryption_state(None);
        let (encryption_session, session_tracker) = make_encryption_session(None);

        let keyslot = KeySlot::draft_v1(KeyScope {
            profile_id: "profile-joiner".to_string(),
        })
        .expect("draft keyslot")
        .finalize(WrappedMasterKey {
            blob: EncryptedBlob {
                version: EncryptionFormatVersion::V1,
                aead: EncryptionAlgo::XChaCha20Poly1305,
                nonce: vec![0u8; 24],
                ciphertext: vec![1u8; 32],
                aad_fingerprint: None,
            },
        });
        let keyslot_blob = serde_json::to_vec(&keyslot).expect("serialize keyslot");

        let adapter = SpaceAccessCryptoAdapter::new(
            SecretString::from("unused"),
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(make_key_scope()),
            Arc::new(encryption_state),
            Arc::new(encryption_session),
        );

        let result = adapter
            .derive_master_key_from_keyslot(&keyslot_blob, SecretString::from("joiner-pass"))
            .await;

        assert!(result.is_ok(), "expected key derivation success");
        assert_eq!(
            result.expect("master key").as_bytes(),
            expected_master_key.as_bytes()
        );

        let km_guard = km_tracker.lock().unwrap();
        assert!(km_guard.store_kek_called, "expected KEK to be stored");
        assert!(
            km_guard.store_keyslot_called,
            "expected keyslot to be stored"
        );
        drop(km_guard);

        let session_guard = session_tracker.lock().unwrap();
        assert!(
            session_guard.set_master_key_called,
            "expected session master key to be set"
        );
        drop(session_guard);

        let state_guard = state_tracker.lock().unwrap();
        assert!(
            state_guard.persist_initialized_called,
            "expected encryption initialization to be persisted"
        );
    }

    #[tokio::test]
    async fn derive_master_key_from_keyslot_rolls_back_when_persist_initialized_fails() {
        let expected_master_key = MasterKey::from_bytes(&[8u8; 32]).expect("valid master key");
        let encryption = make_encryption_ok(Some(expected_master_key));
        let (key_material, km_tracker) = make_key_material(None, None);
        let (encryption_state, _) =
            make_encryption_state(Some(EncryptionStateError::PersistError("boom".into())));
        let (encryption_session, session_tracker) = make_encryption_session(None);

        let keyslot = KeySlot::draft_v1(KeyScope {
            profile_id: "profile-joiner".to_string(),
        })
        .expect("draft keyslot")
        .finalize(WrappedMasterKey {
            blob: EncryptedBlob {
                version: EncryptionFormatVersion::V1,
                aead: EncryptionAlgo::XChaCha20Poly1305,
                nonce: vec![0u8; 24],
                ciphertext: vec![2u8; 32],
                aad_fingerprint: None,
            },
        });
        let keyslot_blob = serde_json::to_vec(&keyslot).expect("serialize keyslot");

        let adapter = SpaceAccessCryptoAdapter::new(
            SecretString::from("unused"),
            Arc::new(encryption),
            Arc::new(key_material),
            Arc::new(make_key_scope()),
            Arc::new(encryption_state),
            Arc::new(encryption_session),
        );

        let result = adapter
            .derive_master_key_from_keyslot(&keyslot_blob, SecretString::from("joiner-pass"))
            .await;

        assert!(result.is_err(), "expected derive failure");
        let km_guard = km_tracker.lock().unwrap();
        assert!(km_guard.delete_kek_called, "expected KEK rollback");
        assert!(km_guard.delete_keyslot_called, "expected keyslot rollback");
        drop(km_guard);

        let session_guard = session_tracker.lock().unwrap();
        assert!(
            session_guard.clear_called,
            "expected encryption session clear rollback"
        );
    }
}
