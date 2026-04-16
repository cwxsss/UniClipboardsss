use std::sync::Arc;

use tokio::sync::Mutex;

use uc_core::ids::SpaceId;
use uc_core::ports::space::{CryptoPort, PersistencePort, ProofPort, SpaceAccessTransportPort};
use uc_core::ports::TimerPort;
use uc_core::security::space_access::state::SpaceAccessState;
use uc_core::security::SecretString;

use super::executor::SpaceAccessExecutor;
use super::orchestrator::{SpaceAccessError, SpaceAccessOrchestrator};

#[derive(Debug, thiserror::Error)]
pub enum StartSponsorAuthorizationError {
    #[error("space access failed: {0}")]
    SpaceAccess(#[from] SpaceAccessError),
}

pub trait SpaceAccessCryptoFactory: Send + Sync {
    fn build(&self, passphrase: SecretString) -> Box<dyn CryptoPort>;
}

pub struct StartSponsorAuthorization {
    orchestrator: Arc<SpaceAccessOrchestrator>,
    crypto_factory: Arc<dyn SpaceAccessCryptoFactory>,
    transport: Arc<Mutex<dyn SpaceAccessTransportPort>>,
    proof: Arc<dyn ProofPort>,
    timer: Arc<Mutex<dyn TimerPort>>,
    store: Arc<Mutex<dyn PersistencePort>>,
    ttl_secs: u64,
}

impl StartSponsorAuthorization {
    pub fn new(
        orchestrator: Arc<SpaceAccessOrchestrator>,
        crypto_factory: Arc<dyn SpaceAccessCryptoFactory>,
        transport: Arc<Mutex<dyn SpaceAccessTransportPort>>,
        proof: Arc<dyn ProofPort>,
        timer: Arc<Mutex<dyn TimerPort>>,
        store: Arc<Mutex<dyn PersistencePort>>,
    ) -> Self {
        Self {
            orchestrator,
            crypto_factory,
            transport,
            proof,
            timer,
            store,
            ttl_secs: 0,
        }
    }

    pub async fn execute(
        &self,
        passphrase: SecretString,
    ) -> Result<SpaceAccessState, StartSponsorAuthorizationError> {
        let space_id = SpaceId::new();
        let pairing_session_id = format!("setup-{}", uuid::Uuid::new_v4());
        let crypto = self.crypto_factory.build(passphrase);
        let mut timer = self.timer.lock().await;
        let mut store = self.store.lock().await;
        let mut transport = self.transport.lock().await;
        let mut executor = SpaceAccessExecutor {
            crypto: crypto.as_ref(),
            transport: &mut *transport,
            proof: self.proof.as_ref(),
            timer: &mut *timer,
            store: &mut *store,
        };

        let state = self
            .orchestrator
            .start_sponsor_authorization(&mut executor, pairing_session_id, space_id, self.ttl_secs)
            .await?;

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{
        MockSpaceAccessCrypto, MockSpaceAccessPersistence, MockSpaceAccessProof,
        MockSpaceAccessTransport, MockTimer,
    };
    use mockall::mock;
    use std::sync::atomic::{AtomicBool, Ordering};
    use uc_core::security::model::{
        EncryptedBlob, EncryptionAlgo, EncryptionFormatVersion, KeyScope, KeySlot, WrappedMasterKey,
    };
    use uc_core::security::{MasterKey, SecretString};

    mock! {
        CryptoFactory {}

        impl SpaceAccessCryptoFactory for CryptoFactory {
            fn build(&self, passphrase: SecretString) -> Box<dyn CryptoPort>;
        }
    }

    #[tokio::test]
    async fn start_sponsor_authorization_exports_keyslot() {
        let exported = Arc::new(AtomicBool::new(false));
        let exported_clone = exported.clone();
        let mut crypto_factory = MockCryptoFactory::new();
        crypto_factory.expect_build().returning(move |_| {
            let mut crypto = MockSpaceAccessCrypto::new();
            crypto.expect_generate_nonce32().returning(|| [7u8; 32]);

            let exported = exported_clone.clone();
            crypto.expect_export_keyslot_blob().returning(move |_| {
                exported.store(true, Ordering::SeqCst);
                let draft = KeySlot::draft_v1(KeyScope {
                    profile_id: "test".to_string(),
                })?;
                Ok(draft.finalize(WrappedMasterKey {
                    blob: EncryptedBlob {
                        version: EncryptionFormatVersion::V1,
                        aead: EncryptionAlgo::XChaCha20Poly1305,
                        nonce: vec![0u8; 24],
                        ciphertext: vec![1u8; 32],
                        aad_fingerprint: None,
                    },
                }))
            });
            crypto
                .expect_derive_master_key_from_keyslot()
                .returning(|_, _| {
                    MasterKey::from_bytes(&[0u8; 32]).map_err(|e| anyhow::anyhow!(e))
                });

            Box::new(crypto)
        });
        let crypto_factory = Arc::new(crypto_factory);

        let mut transport = MockSpaceAccessTransport::new();
        transport.expect_send_offer().returning(|_| Ok(()));
        transport.expect_send_proof().returning(|_| Ok(()));
        transport.expect_send_result().returning(|_| Ok(()));

        let mut proof = MockSpaceAccessProof::new();
        proof
            .expect_build_proof()
            .returning(|sid, space_id, nonce, _| {
                Ok(uc_core::security::space_access::SpaceAccessProofArtifact {
                    pairing_session_id: sid.clone(),
                    space_id: space_id.clone(),
                    challenge_nonce: nonce,
                    proof_bytes: vec![],
                })
            });
        proof.expect_verify_proof().returning(|_, _| Ok(true));

        let mut timer = MockTimer::new();
        timer.expect_start().returning(|_, _| Ok(()));
        timer.expect_stop().returning(|_| Ok(()));

        let mut store = MockSpaceAccessPersistence::new();
        store
            .expect_persist_sponsor_access()
            .returning(|_, _| Ok(()));
        store
            .expect_persist_joiner_access()
            .returning(|_, _| Ok(()));

        let orchestrator = Arc::new(SpaceAccessOrchestrator::new());

        let uc = StartSponsorAuthorization::new(
            orchestrator,
            crypto_factory,
            Arc::new(Mutex::new(transport)),
            Arc::new(proof),
            Arc::new(Mutex::new(timer)),
            Arc::new(Mutex::new(store)),
        );

        let result = uc.execute(SecretString::from("passphrase")).await;

        assert!(
            result.is_ok(),
            "expected sponsor authorization start to succeed"
        );
        assert!(exported.load(Ordering::SeqCst));
    }
}
