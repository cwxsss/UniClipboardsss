use anyhow::Result;

use uc_application::pairing::{PairingDomainEvent, PairingEventPort, PairingFacade};

#[async_trait::async_trait]
pub trait SetupPairingFacadePort: Send + Sync {
    async fn subscribe(&self) -> Result<tokio::sync::mpsc::Receiver<PairingDomainEvent>>;
    async fn initiate_pairing(&self, peer_id: String) -> Result<String>;
    async fn accept_pairing(&self, session_id: &str) -> Result<()>;
    async fn reject_pairing(&self, session_id: &str) -> Result<()>;
    async fn cancel_pairing(&self, session_id: &str) -> Result<()>;
    async fn verify_pairing(&self, session_id: &str, pin_matches: bool) -> Result<()>;
}

#[async_trait::async_trait]
impl SetupPairingFacadePort for PairingFacade {
    async fn subscribe(&self) -> Result<tokio::sync::mpsc::Receiver<PairingDomainEvent>> {
        PairingEventPort::subscribe(self).await
    }

    async fn initiate_pairing(&self, peer_id: String) -> Result<String> {
        self.initiate_pairing(peer_id).await
    }

    async fn accept_pairing(&self, session_id: &str) -> Result<()> {
        self.accept_pairing(session_id).await
    }

    async fn reject_pairing(&self, session_id: &str) -> Result<()> {
        self.reject_pairing(session_id).await
    }

    async fn cancel_pairing(&self, session_id: &str) -> Result<()> {
        self.cancel_pairing(session_id).await
    }

    async fn verify_pairing(&self, session_id: &str, pin_matches: bool) -> Result<()> {
        self.verify_pairing(session_id, pin_matches).await
    }
}
