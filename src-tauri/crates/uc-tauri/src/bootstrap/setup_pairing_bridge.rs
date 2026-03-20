//! Daemon-backed setup pairing facade.
//!
//! This facade provides the setup flow with pairing capabilities by wrapping
//! the daemon pairing client. It allows setup to initiate, accept, and reject
//! pairing sessions without depending on the concrete PairingOrchestrator type.

use std::sync::Arc;

use anyhow::Result;
use tracing::error;

use crate::bootstrap::DaemonConnectionState;
use crate::daemon_client::TauriDaemonPairingClient;

/// Port trait for setup pairing facade operations.
///
/// This trait defines the interface needed by the setup flow for pairing operations.
#[async_trait::async_trait]
pub trait SetupPairingFacadePort: Send + Sync {
    /// Subscribe to pairing domain events.
    /// Returns a channel receiver for receiving pairing events.
    async fn subscribe(
        &self,
    ) -> Result<tokio::sync::mpsc::Receiver<uc_app::usecases::pairing::PairingDomainEvent>>;

    /// Initiate pairing with a peer.
    async fn initiate_pairing(&self, peer_id: String) -> Result<String>;

    /// Accept an incoming pairing request.
    async fn accept_pairing(&self, session_id: &str) -> Result<()>;

    /// Reject an incoming pairing request.
    async fn reject_pairing(&self, session_id: &str) -> Result<()>;

    /// Cancel an active pairing session.
    async fn cancel_pairing(&self, session_id: &str) -> Result<()>;

    /// Verify pairing PIN.
    async fn verify_pairing(&self, session_id: &str, pin_matches: bool) -> Result<()>;
}

/// Daemon-backed setup pairing facade.
///
/// This facade provides the subset of pairing operations needed by the setup flow:
/// - Subscribe to pairing events
/// - Initiate pairing
/// - Accept pairing
/// - Reject pairing
pub struct DaemonBackedSetupPairingFacade {
    /// Daemon connection state.
    connection_state: DaemonConnectionState,
    /// Flag indicating if participant-ready is active.
    participant_ready: bool,
}

impl DaemonBackedSetupPairingFacade {
    /// Create a new daemon-backed setup pairing facade.
    pub fn new(connection_state: DaemonConnectionState) -> Self {
        Self {
            connection_state,
            participant_ready: false,
        }
    }

    /// Subscribe to pairing domain events from the daemon.
    ///
    /// Returns a receiver channel for receiving pairing events.
    /// Note: This is a placeholder - full implementation would connect to daemon WebSocket.
    pub async fn subscribe(
        &self,
    ) -> Result<tokio::sync::mpsc::Receiver<uc_app::usecases::pairing::PairingDomainEvent>> {
        // For now, we return a disconnected channel. Full implementation would
        // connect to the daemon WebSocket and translate events.
        let (_tx, rx) = tokio::sync::mpsc::channel(32);
        Ok(rx)
    }

    /// Initiate a pairing session with a peer.
    pub async fn initiate_pairing(&self, peer_id: String) -> Result<String> {
        let client = TauriDaemonPairingClient::new(self.connection_state.clone());
        let response = client.initiate_pairing(peer_id).await?;

        if response.accepted {
            Ok(response.session_id)
        } else {
            Err(anyhow::anyhow!(
                "failed to initiate pairing: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            ))
        }
    }

    /// Accept an incoming pairing request.
    pub async fn accept_pairing(&self, session_id: &str) -> Result<()> {
        let client = TauriDaemonPairingClient::new(self.connection_state.clone());
        client.accept_pairing(session_id).await?;
        Ok(())
    }

    /// Reject an incoming pairing request.
    pub async fn reject_pairing(&self, session_id: &str) -> Result<()> {
        let client = TauriDaemonPairingClient::new(self.connection_state.clone());
        client.reject_pairing(session_id).await?;
        Ok(())
    }

    /// Cancel an active pairing session.
    pub async fn cancel_pairing(&self, session_id: &str) -> Result<()> {
        let client = TauriDaemonPairingClient::new(self.connection_state.clone());
        client.cancel_pairing(session_id).await?;
        Ok(())
    }

    /// Verify pairing PIN.
    pub async fn verify_pairing(&self, session_id: &str, pin_matches: bool) -> Result<()> {
        let client = TauriDaemonPairingClient::new(self.connection_state.clone());
        client.verify_pairing(session_id, pin_matches).await?;
        Ok(())
    }

    /// Set participant ready status.
    pub async fn set_participant_ready(
        &mut self,
        ready: bool,
        lease_ttl_ms: Option<u64>,
    ) -> Result<()> {
        let client = TauriDaemonPairingClient::new(self.connection_state.clone());
        client
            .set_pairing_participant_ready("setup", ready, lease_ttl_ms)
            .await?;
        self.participant_ready = ready;
        Ok(())
    }

    /// Check if participant ready is active.
    pub fn is_participant_ready(&self) -> bool {
        self.participant_ready
    }
}

impl Drop for DaemonBackedSetupPairingFacade {
    fn drop(&mut self) {
        // Revoke participant ready on drop.
        if self.participant_ready {
            let connection_state = self.connection_state.clone();
            tokio::spawn(async move {
                let client = TauriDaemonPairingClient::new(connection_state);
                if let Err(e) = client
                    .set_pairing_participant_ready("setup", false, None)
                    .await
                {
                    error!(error = %e, "failed to revoke participant-ready on facade drop");
                }
            });
        }
    }
}

#[async_trait::async_trait]
impl SetupPairingFacadePort for DaemonBackedSetupPairingFacade {
    async fn subscribe(
        &self,
    ) -> Result<tokio::sync::mpsc::Receiver<uc_app::usecases::pairing::PairingDomainEvent>> {
        self.subscribe().await
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

/// Build a setup pairing facade from connection state.
pub fn build_setup_pairing_facade(
    connection_state: DaemonConnectionState,
) -> Arc<dyn SetupPairingFacadePort> {
    Arc::new(DaemonBackedSetupPairingFacade::new(connection_state))
}
