#![allow(deprecated)] // legacy NetworkEventPort subscription; replaced in Slice 5

//! # PeerMonitor
//!
//! Dedicated [`DaemonService`] that subscribes to network events and emits
//! peer lifecycle WebSocket events. Extracted from `DaemonPairingHost` so that
//! peer event handling and pairing protocol logic are cleanly separated.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::CoreUseCases;
use uc_core::network::NetworkEvent;

use crate::api::projection::IntoApiDto;
use crate::api::types::{
    DaemonWsEvent, PeerConnectionChangedPayload, PeerNameUpdatedPayload, PeerSnapshotDto,
    PeersChangedFullPayload,
};
use crate::service::{DaemonService, ServiceHealth};

const PEER_EVENTS_SUBSCRIBE_BACKOFF_INITIAL_MS: u64 = 250;
const PEER_EVENTS_SUBSCRIBE_BACKOFF_MAX_MS: u64 = 30_000;

fn peer_events_subscribe_backoff_ms(attempt: u32) -> u64 {
    let exponent = attempt.saturating_sub(1).min(16);
    let factor = 1u64 << exponent;
    PEER_EVENTS_SUBSCRIBE_BACKOFF_INITIAL_MS
        .saturating_mul(factor)
        .min(PEER_EVENTS_SUBSCRIBE_BACKOFF_MAX_MS)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn emit_ws_event<T: serde::Serialize>(
    event_tx: &broadcast::Sender<DaemonWsEvent>,
    topic: &str,
    event_type: &str,
    session_id: Option<String>,
    payload: T,
) {
    let payload = match serde_json::to_value(payload) {
        Ok(payload) => payload,
        Err(err) => {
            warn!(error = %err, topic, event_type, "failed to encode daemon websocket payload");
            return;
        }
    };

    let _ = event_tx.send(DaemonWsEvent {
        topic: topic.to_string(),
        event_type: event_type.to_string(),
        session_id,
        ts: now_ms(),
        payload,
    });
}

/// Monitors peer lifecycle network events and emits corresponding WebSocket events.
///
/// Handles: `PeerDiscovered`, `PeerLost`, `PeerNameUpdated`, `PeerConnected`, `PeerDisconnected`.
/// All other network events are ignored (pairing events are handled by `DaemonPairingHost`).
pub struct PeerMonitor {
    runtime: Arc<CoreRuntime>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl PeerMonitor {
    pub fn new(runtime: Arc<CoreRuntime>, event_tx: broadcast::Sender<DaemonWsEvent>) -> Self {
        Self { runtime, event_tx }
    }

    async fn run_peer_event_loop(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        let network_events = self.runtime.wiring_deps().network_ports.events.clone();

        let mut subscribe_attempt: u32 = 0;
        loop {
            let subscribe_result = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = network_events.subscribe_events() => result,
            };

            match subscribe_result {
                Ok(mut event_rx) => {
                    subscribe_attempt = 0;
                    loop {
                        tokio::select! {
                            _ = cancel.cancelled() => return Ok(()),
                            maybe_event = event_rx.recv() => {
                                let Some(event) = maybe_event else {
                                    break;
                                };

                                match event {
                                    NetworkEvent::PeerDiscovered(_peer) => {
                                        let usecases = CoreUseCases::new(self.runtime.as_ref());
                                        match usecases.get_p2p_peers_snapshot().execute().await {
                                            Ok(snapshots) => {
                                                let peers: Vec<PeerSnapshotDto> = snapshots
                                                    .into_iter()
                                                    .map(IntoApiDto::into_api_dto)
                                                    .collect();
                                                emit_ws_event(
                                                    &self.event_tx,
                                                    "peers",
                                                    "peers.changed",
                                                    None,
                                                    PeersChangedFullPayload { peers },
                                                );
                                            }
                                            Err(e) => {
                                                warn!(
                                                    error = %e,
                                                    "failed to fetch peer snapshot on PeerDiscovered"
                                                );
                                            }
                                        }
                                    }
                                    NetworkEvent::PeerLost(_peer_id) => {
                                        let usecases = CoreUseCases::new(self.runtime.as_ref());
                                        match usecases.get_p2p_peers_snapshot().execute().await {
                                            Ok(snapshots) => {
                                                let peers: Vec<PeerSnapshotDto> = snapshots
                                                    .into_iter()
                                                    .map(IntoApiDto::into_api_dto)
                                                    .collect();
                                                emit_ws_event(
                                                    &self.event_tx,
                                                    "peers",
                                                    "peers.changed",
                                                    None,
                                                    PeersChangedFullPayload { peers },
                                                );
                                            }
                                            Err(e) => {
                                                warn!(
                                                    error = %e,
                                                    "failed to fetch peer snapshot on PeerLost"
                                                );
                                            }
                                        }
                                    }
                                    NetworkEvent::PeerNameUpdated { peer_id, device_name } => {
                                        emit_ws_event(
                                            &self.event_tx,
                                            "peers",
                                            "peers.name_updated",
                                            None,
                                            PeerNameUpdatedPayload { peer_id, device_name },
                                        );
                                    }
                                    NetworkEvent::PeerConnected(peer) => {
                                        emit_ws_event(
                                            &self.event_tx,
                                            "peers",
                                            "peers.connection_changed",
                                            None,
                                            PeerConnectionChangedPayload {
                                                peer_id: peer.peer_id,
                                                device_name: Some(peer.device_name),
                                                connected: true,
                                            },
                                        );
                                    }
                                    NetworkEvent::PeerDisconnected(peer_id) => {
                                        emit_ws_event(
                                            &self.event_tx,
                                            "peers",
                                            "peers.connection_changed",
                                            None,
                                            PeerConnectionChangedPayload {
                                                peer_id,
                                                device_name: None,
                                                connected: false,
                                            },
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    subscribe_attempt = subscribe_attempt.saturating_add(1);
                    let retry_in_ms = peer_events_subscribe_backoff_ms(subscribe_attempt);
                    warn!(
                        error = %err,
                        attempt = subscribe_attempt,
                        retry_in_ms,
                        "failed to subscribe to peer network events"
                    );
                }
            }

            let backoff =
                Duration::from_millis(peer_events_subscribe_backoff_ms(subscribe_attempt));
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = tokio::time::sleep(backoff) => {}
            }
        }
    }
}

#[async_trait]
impl DaemonService for PeerMonitor {
    fn name(&self) -> &str {
        "peer-monitor"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        info!("peer monitor starting");
        self.run_peer_event_loop(cancel).await
    }

    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}
