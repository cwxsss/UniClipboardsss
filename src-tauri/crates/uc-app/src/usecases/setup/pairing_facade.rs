use anyhow::Result;

use crate::realtime::SetupPairingEventHub;
use crate::usecases::pairing::{PairingDomainEvent, PairingEventPort, PairingOrchestrator};

#[async_trait::async_trait]
pub trait SetupPairingFacadePort: Send + Sync {
    async fn subscribe(&self) -> Result<tokio::sync::mpsc::Receiver<PairingDomainEvent>>;
    async fn initiate_pairing(&self, peer_id: String) -> Result<String>;
    async fn accept_pairing(&self, session_id: &str) -> Result<()>;
    async fn reject_pairing(&self, session_id: &str) -> Result<()>;
    async fn cancel_pairing(&self, session_id: &str) -> Result<()>;
    async fn verify_pairing(&self, session_id: &str, pin_matches: bool) -> Result<()>;
}

#[allow(dead_code)]
pub type SetupPairingSubscriptionHub = SetupPairingEventHub;

#[async_trait::async_trait]
impl SetupPairingFacadePort for PairingOrchestrator {
    async fn subscribe(&self) -> Result<tokio::sync::mpsc::Receiver<PairingDomainEvent>> {
        PairingEventPort::subscribe(self).await
    }

    async fn initiate_pairing(&self, peer_id: String) -> Result<String> {
        self.initiate_pairing(peer_id).await
    }

    async fn accept_pairing(&self, session_id: &str) -> Result<()> {
        self.user_accept_pairing(session_id).await
    }

    async fn reject_pairing(&self, session_id: &str) -> Result<()> {
        self.user_reject_pairing(session_id).await
    }

    async fn cancel_pairing(&self, session_id: &str) -> Result<()> {
        self.user_reject_pairing(session_id).await
    }

    async fn verify_pairing(&self, session_id: &str, pin_matches: bool) -> Result<()> {
        if pin_matches {
            self.user_accept_pairing(session_id).await
        } else {
            self.user_reject_pairing(session_id).await
        }
    }
}
