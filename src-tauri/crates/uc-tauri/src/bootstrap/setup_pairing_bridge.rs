//! Daemon-backed setup pairing facade.
//!
//! This facade provides the setup flow with pairing capabilities by wrapping
//! the daemon pairing client. It allows setup to initiate, accept, and reject
//! pairing sessions without depending on the concrete PairingOrchestrator type.

use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{http::Request, Message};
use tracing::{debug, error, warn};

use crate::bootstrap::DaemonConnectionState;
use crate::daemon_client::TauriDaemonPairingClient;
use uc_app::realtime::SetupPairingEventHub;
use uc_app::usecases::pairing::PairingDomainEvent;
use uc_app::usecases::setup::SetupPairingFacadePort;
use uc_core::network::pairing_state_machine::FailureReason;

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
    /// Shared setup realtime hub fed by the unified daemon bridge when available.
    event_hub: Option<Arc<SetupPairingEventHub>>,
    /// Flag indicating if participant-ready is active.
    participant_ready: bool,
}

impl DaemonBackedSetupPairingFacade {
    /// Create a new daemon-backed setup pairing facade.
    pub fn new(connection_state: DaemonConnectionState) -> Self {
        Self::with_event_hub(connection_state, None)
    }

    pub fn with_event_hub(
        connection_state: DaemonConnectionState,
        event_hub: Option<Arc<SetupPairingEventHub>>,
    ) -> Self {
        Self {
            connection_state,
            event_hub,
            participant_ready: false,
        }
    }

    /// Subscribe to pairing domain events from the daemon.
    ///
    /// Returns a receiver channel for receiving pairing events.
    pub async fn subscribe(&self) -> Result<mpsc::Receiver<PairingDomainEvent>> {
        if let Some(event_hub) = &self.event_hub {
            return event_hub.subscribe().await;
        }

        self.subscribe_via_websocket().await
    }

    async fn subscribe_via_websocket(&self) -> Result<mpsc::Receiver<PairingDomainEvent>> {
        let connection = self
            .connection_state
            .get()
            .context("daemon connection not available for setup pairing subscription")?;
        let request = Request::builder()
            .uri(connection.ws_url)
            .header("Authorization", format!("Bearer {}", connection.token))
            .body(())?;

        let (ws_stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .context("failed to connect setup pairing websocket")?;
        let (mut write, mut read) = ws_stream.split();
        let subscribe_request = serde_json::json!({
            "action": "subscribe",
            "topics": ["pairing"]
        });

        write
            .send(Message::Text(subscribe_request.to_string().into()))
            .await
            .context("failed to subscribe to pairing websocket topic")?;

        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(async move {
            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        if let Some(event) = map_daemon_ws_event(&text) {
                            if tx.send(event).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(err) => {
                        warn!(error = %err, "setup pairing websocket receive failed");
                        break;
                    }
                }
            }
            debug!("setup pairing websocket listener stopped");
        });

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
    async fn subscribe(&self) -> Result<mpsc::Receiver<PairingDomainEvent>> {
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

fn map_daemon_ws_event(text: &str) -> Option<PairingDomainEvent> {
    let event: uc_daemon::api::types::DaemonWsEvent = match serde_json::from_str(text) {
        Ok(event) => event,
        Err(err) => {
            warn!(error = %err, "failed to parse setup pairing websocket event");
            return None;
        }
    };

    match event.event_type.as_str() {
        "pairing.verification_required" => {
            let payload: uc_daemon::api::types::PairingVerificationPayload =
                match serde_json::from_value(event.payload) {
                    Ok(payload) => payload,
                    Err(err) => {
                        warn!(error = %err, "failed to decode pairing verification payload");
                        return None;
                    }
                };

            Some(PairingDomainEvent::PairingVerificationRequired {
                session_id: payload.session_id,
                peer_id: payload.peer_id,
                short_code: payload.code,
                local_fingerprint: payload.local_fingerprint,
                peer_fingerprint: payload.peer_fingerprint,
            })
        }
        "pairing.complete" => {
            let payload: uc_daemon::api::types::PairingSessionChangedPayload =
                match serde_json::from_value(event.payload) {
                    Ok(payload) => payload,
                    Err(err) => {
                        warn!(error = %err, "failed to decode pairing complete payload");
                        return None;
                    }
                };

            Some(PairingDomainEvent::PairingSucceeded {
                session_id: payload.session_id,
                peer_id: payload.peer_id.unwrap_or_default(),
            })
        }
        "pairing.failed" => {
            let payload: uc_daemon::api::types::PairingFailurePayload =
                match serde_json::from_value(event.payload) {
                    Ok(payload) => payload,
                    Err(err) => {
                        warn!(error = %err, "failed to decode pairing failed payload");
                        return None;
                    }
                };

            Some(PairingDomainEvent::PairingFailed {
                session_id: payload.session_id,
                peer_id: payload.peer_id.unwrap_or_default(),
                reason: FailureReason::Other(payload.error),
            })
        }
        _ => None,
    }
}
