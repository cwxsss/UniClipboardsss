//! Daemon pairing bridge - translates daemon WebSocket events to frontend Tauri events.
//!
//! This bridge connects to the daemon's WebSocket subscription, listens for pairing,
//! peers, and paired-devices topics, and re-emits them as the existing frontend
//! event names to maintain backward compatibility with the current desktop UI.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::bootstrap::DaemonConnectionState;
use uc_daemon::api::types::DaemonWsEvent;

/// Topics to subscribe to from the daemon WebSocket.
const DAEMON_TOPICS: &[&str] = &["pairing", "peers", "paired-devices"];

/// Frontend event names that must be preserved for backward compatibility.
const EVENT_P2P_PAIRING_VERIFICATION: &str = "p2p-pairing-verification";
const EVENT_P2P_PEER_DISCOVERY_CHANGED: &str = "p2p-peer-discovery-changed";
const EVENT_P2P_PEER_NAME_UPDATED: &str = "p2p-peer-name-updated";
const EVENT_P2P_PEER_CONNECTION_CHANGED: &str = "p2p-peer-connection-changed";
const EVENT_PAIRING_BRIDGE_LEASE_LOST: &str = "pairing-bridge-lease-lost";

/// Daemon WebSocket event types.
const EVENT_PAIRING_VERIFICATION_REQUIRED: &str = "pairing.verification_required";
const EVENT_PAIRING_COMPLETE: &str = "pairing.complete";
const EVENT_PAIRING_FAILED: &str = "pairing.failed";
const EVENT_PEERS_CHANGED: &str = "peers.changed";
const EVENT_PEERS_NAME_UPDATED: &str = "peers.name_updated";
const EVENT_PEERS_CONNECTION_CHANGED: &str = "peers.connection_changed";
#[allow(dead_code)]
const EVENT_PAIRED_DEVICES_CHANGED: &str = "paired-devices.changed";

/// Subscription request format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscribeRequest {
    action: String,
    topics: Vec<String>,
}

/// Discovery peer info for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPeerInfo {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub addresses: Vec<String>,
    pub is_paired: bool,
    pub connected: bool,
}

/// Frontend pairing verification event kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum P2PPairingVerificationKind {
    Request,
    Verification,
    Verifying,
    Complete,
    Failed,
}

/// Frontend pairing verification event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPairingVerificationEvent {
    pub kind: P2PPairingVerificationKind,
    pub session_id: Option<String>,
    pub peer_id: Option<String>,
    pub short_code: Option<String>,
    pub peer_fingerprint: Option<String>,
    pub error: Option<String>,
}

/// Frontend peer discovery changed event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPeerDiscoveryChangedEvent {
    pub peers: Vec<P2PPeerInfo>,
}

/// Frontend peer name updated event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPeerNameUpdatedEvent {
    pub peer_id: String,
    pub device_name: String,
}

/// Frontend peer connection changed event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPeerConnectionChangedEvent {
    pub peer_id: String,
    pub connected: bool,
}

/// Bridge lease lost event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingBridgeLeaseLostEvent {
    pub reason: String,
    pub can_recover: bool,
}

/// Manages the daemon WebSocket connection and translates events to frontend format.
pub struct PairingBridge {
    /// Tauri app handle for emitting events.
    app_handle: AppHandle,
    /// Daemon connection state for getting connection info.
    connection_state: DaemonConnectionState,
    /// Flag indicating if participant-ready is currently active.
    participant_ready: Arc<RwLock<bool>>,
    /// Flag indicating if discoverability is currently active.
    discoverable: Arc<RwLock<bool>>,
    /// Channel to signal bridge shutdown.
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl PairingBridge {
    /// Create a new pairing bridge.
    pub fn new(app_handle: AppHandle, connection_state: DaemonConnectionState) -> Self {
        Self {
            app_handle,
            connection_state,
            participant_ready: Arc::new(RwLock::new(false)),
            discoverable: Arc::new(RwLock::new(false)),
            shutdown_tx: None,
        }
    }

    /// Start the bridge - connect to daemon WebSocket and process events.
    pub async fn start(&mut self) {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let app_handle = self.app_handle.clone();
        let connection_state = self.connection_state.clone();
        let participant_ready = self.participant_ready.clone();
        let discoverable = self.discoverable.clone();

        tokio::spawn(async move {
            loop {
                // Check for shutdown signal.
                if shutdown_rx.try_recv().is_ok() {
                    info!("Pairing bridge shutdown requested");
                    break;
                }

                // Get daemon connection info.
                let connection = match connection_state.get() {
                    Some(conn) => conn,
                    None => {
                        warn!("Daemon connection not available, retrying in 1s");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };

                // Connect to WebSocket.
                let ws_url = connection.ws_url.clone();
                let token = connection.token.clone();

                match Self::connect_and_subscribe(
                    &app_handle,
                    &ws_url,
                    &token,
                    participant_ready.clone(),
                    discoverable.clone(),
                    &mut shutdown_rx,
                )
                .await
                {
                    Ok(_) => {
                        info!("Daemon WebSocket connection closed normally");
                    }
                    Err(e) => {
                        error!(error = %e, "Daemon WebSocket connection failed, retrying in 2s");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }

                // Check for shutdown before retrying.
                if shutdown_rx.try_recv().is_ok() {
                    info!("Pairing bridge shutdown requested during reconnect");
                    break;
                }
            }

            // On shutdown, revoke discoverability and participant-ready.
            Self::revoke_all(&app_handle, &connection_state).await;
        });
    }

    /// Stop the bridge.
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }

    /// Set participant ready status - called when GUI pairing flows are active.
    pub async fn set_participant_ready(
        &self,
        ready: bool,
        lease_ttl_ms: Option<u64>,
    ) -> Result<()> {
        let client =
            crate::daemon_client::TauriDaemonPairingClient::new(self.connection_state.clone());
        client
            .set_pairing_participant_ready("gui", ready, lease_ttl_ms)
            .await?;

        *self.participant_ready.write().await = ready;
        Ok(())
    }

    /// Set discoverability status - called during GUI startup.
    pub async fn set_discoverable(
        &self,
        discoverable: bool,
        lease_ttl_ms: Option<u64>,
    ) -> Result<()> {
        let client =
            crate::daemon_client::TauriDaemonPairingClient::new(self.connection_state.clone());
        client
            .set_pairing_discoverability("gui", discoverable, lease_ttl_ms)
            .await?;

        *self.discoverable.write().await = discoverable;
        Ok(())
    }

    /// Connect to daemon WebSocket and process events.
    async fn connect_and_subscribe(
        app_handle: &AppHandle,
        ws_url: &str,
        _token: &str,
        _participant_ready: Arc<RwLock<bool>>,
        _discoverable: Arc<RwLock<bool>>,
        shutdown_rx: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let url = format!("{}ws", ws_url.trim_end_matches('/'));
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .context("failed to connect to daemon WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // Send subscription request.
        let subscribe_request = SubscribeRequest {
            action: "subscribe".to_string(),
            topics: DAEMON_TOPICS.iter().map(|s| s.to_string()).collect(),
        };
        let request_json = serde_json::to_string(&subscribe_request)?;
        write
            .send(Message::Text(request_json.into()))
            .await
            .context("failed to send subscription request")?;

        info!(topics = ?DAEMON_TOPICS, "subscribed to daemon pairing topics");

        // Process incoming messages.
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("Bridge shutdown received");
                    break;
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = Self::handle_message(app_handle, &text).await {
                                warn!(error = %e, "failed to handle WebSocket message");
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            debug!("Daemon WebSocket closed by server");
                            break;
                        }
                        Some(Err(e)) => {
                            warn!(error = %e, "WebSocket error");
                            break;
                        }
                        None => {
                            debug!("WebSocket stream ended");
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle incoming WebSocket message.
    async fn handle_message(app_handle: &AppHandle, text: &str) -> Result<()> {
        let event: DaemonWsEvent =
            serde_json::from_str(text).context("failed to parse daemon WebSocket event")?;

        debug!(topic = %event.topic, event_type = %event.event_type, "received daemon event");

        match event.topic.as_str() {
            "pairing" => {
                Self::handle_pairing_event(app_handle, &event).await?;
            }
            "peers" => {
                Self::handle_peers_event(app_handle, &event).await?;
            }
            "paired-devices" => {
                // Paired devices changes don't emit frontend events directly -
                // they're reflected in the peers snapshot.
                debug!(event_type = %event.event_type, "paired-devices event received");
            }
            _ => {
                warn!(topic = %event.topic, "unknown daemon topic");
            }
        }

        Ok(())
    }

    /// Handle pairing-related events.
    async fn handle_pairing_event(app_handle: &AppHandle, event: &DaemonWsEvent) -> Result<()> {
        match event.event_type.as_str() {
            EVENT_PAIRING_VERIFICATION_REQUIRED => {
                let payload = event.payload.clone();
                let session_id = payload
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let peer_id = payload
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let short_code = payload
                    .get("shortCode")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let peer_fingerprint = payload
                    .get("peerFingerprint")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                // Determine kind based on session state.
                let state = payload
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("request");

                let kind = match state {
                    "request" => P2PPairingVerificationKind::Request,
                    "verification" => P2PPairingVerificationKind::Verification,
                    "verifying" => P2PPairingVerificationKind::Verifying,
                    _ => P2PPairingVerificationKind::Verification,
                };

                let frontend_event = P2PPairingVerificationEvent {
                    kind,
                    session_id,
                    peer_id,
                    short_code,
                    peer_fingerprint,
                    error: None,
                };

                app_handle
                    .emit(EVENT_P2P_PAIRING_VERIFICATION, frontend_event)
                    .map_err(|e| anyhow!("failed to emit p2p-pairing-verification: {}", e))?;
            }
            EVENT_PAIRING_COMPLETE => {
                let payload = event.payload.clone();
                let session_id = payload
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let peer_id = payload
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let frontend_event = P2PPairingVerificationEvent {
                    kind: P2PPairingVerificationKind::Complete,
                    session_id,
                    peer_id,
                    short_code: None,
                    peer_fingerprint: None,
                    error: None,
                };

                app_handle
                    .emit(EVENT_P2P_PAIRING_VERIFICATION, frontend_event)
                    .map_err(|e| anyhow!("failed to emit p2p-pairing-verification: {}", e))?;
            }
            EVENT_PAIRING_FAILED => {
                let payload = event.payload.clone();
                let session_id = payload
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let error = payload
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let frontend_event = P2PPairingVerificationEvent {
                    kind: P2PPairingVerificationKind::Failed,
                    session_id,
                    peer_id: None,
                    short_code: None,
                    peer_fingerprint: None,
                    error,
                };

                app_handle
                    .emit(EVENT_P2P_PAIRING_VERIFICATION, frontend_event)
                    .map_err(|e| anyhow!("failed to emit p2p-pairing-verification: {}", e))?;
            }
            _ => {
                debug!(event_type = %event.event_type, "unhandled pairing event type");
            }
        }

        Ok(())
    }

    /// Handle peers-related events.
    async fn handle_peers_event(app_handle: &AppHandle, event: &DaemonWsEvent) -> Result<()> {
        match event.event_type.as_str() {
            EVENT_PEERS_CHANGED => {
                let payload = event.payload.clone();
                let peers: Vec<P2PPeerInfo> = payload
                    .get("peers")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| {
                                Some(P2PPeerInfo {
                                    peer_id: p.get("peerId")?.as_str()?.to_string(),
                                    device_name: p
                                        .get("deviceName")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                    addresses: p
                                        .get("addresses")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|a| a.as_str().map(String::from))
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    is_paired: p
                                        .get("isPaired")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false),
                                    connected: p
                                        .get("connected")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let frontend_event = P2PPeerDiscoveryChangedEvent { peers };
                app_handle
                    .emit(EVENT_P2P_PEER_DISCOVERY_CHANGED, frontend_event)
                    .map_err(|e| anyhow!("failed to emit p2p-peer-discovery-changed: {}", e))?;
            }
            EVENT_PEERS_NAME_UPDATED => {
                let payload = event.payload.clone();
                let peer_id = payload
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_default();
                let device_name = payload
                    .get("deviceName")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_default();

                let frontend_event = P2PPeerNameUpdatedEvent {
                    peer_id,
                    device_name,
                };
                app_handle
                    .emit(EVENT_P2P_PEER_NAME_UPDATED, frontend_event)
                    .map_err(|e| anyhow!("failed to emit p2p-peer-name-updated: {}", e))?;
            }
            EVENT_PEERS_CONNECTION_CHANGED => {
                let payload = event.payload.clone();
                let peer_id = payload
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_default();
                let connected = payload
                    .get("connected")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let frontend_event = P2PPeerConnectionChangedEvent { peer_id, connected };
                app_handle
                    .emit(EVENT_P2P_PEER_CONNECTION_CHANGED, frontend_event)
                    .map_err(|e| anyhow!("failed to emit p2p-peer-connection-changed: {}", e))?;
            }
            _ => {
                debug!(event_type = %event.event_type, "unhandled peers event type");
            }
        }

        Ok(())
    }

    /// Revoke all discoverability and participant-ready on shutdown.
    async fn revoke_all(app_handle: &AppHandle, connection_state: &DaemonConnectionState) {
        let client = crate::daemon_client::TauriDaemonPairingClient::new(connection_state.clone());

        // Revoke discoverability.
        if let Err(e) = client.set_pairing_discoverability("gui", false, None).await {
            warn!(error = %e, "failed to revoke discoverability on bridge shutdown");
        }

        // Revoke participant ready.
        if let Err(e) = client
            .set_pairing_participant_ready("gui", false, None)
            .await
        {
            warn!(error = %e, "failed to revoke participant-ready on bridge shutdown");
        }

        // Emit lease lost event.
        let event = PairingBridgeLeaseLostEvent {
            reason: "bridge_shutdown".to_string(),
            can_recover: false,
        };
        let _ = app_handle.emit(EVENT_PAIRING_BRIDGE_LEASE_LOST, event);
    }
}

use serde::Serialize;
