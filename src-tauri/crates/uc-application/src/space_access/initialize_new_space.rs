use std::sync::Arc;

use tokio::sync::Mutex;

use uc_core::crypto::SecretString;
use uc_core::ids::{SessionId, SpaceId};
use uc_core::ports::space::{CryptoPort, PersistencePort, ProofPort, SpaceAccessTransportPort};
use uc_core::ports::TimerPort;
use uc_core::space_access::state::SpaceAccessState;

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
        let pairing_session_id = SessionId::from(format!("setup-{}", uuid::Uuid::new_v4()));
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
