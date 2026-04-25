//! Presence monitor — bridges [`PresencePort`] events into the daemon
//! WebSocket as `peers.changed` snapshots.
//!
//! ## Why
//!
//! `PresencePort` (Slice 2 Phase 1) is the iroh-stack signal for
//! reachability changes; its `subscribe()` returns a `broadcast::Receiver`
//! that fires on every Online/Offline transition driven by either the
//! `ensure_reachable` dial path or the per-peer watchdog observing
//! `Connection::closed()`.
//!
//! The frontend's existing `peers` topic subscribers (Setup scan page via
//! `useDeviceDiscovery`) consume the `peers.changed` full-snapshot event
//! and run their own diff. Other historical event types
//! (`peers.connectionChanged`, `peers.nameUpdated`) have no real consumer
//! today, so this monitor intentionally only forwards `peers.changed`.
//!
//! ## Design
//!
//! * Subscribes once at startup; the broadcast channel is process-local
//!   so no reconnect/backoff loop is needed (compare the legacy
//!   `PeerMonitor` which had to retry `NetworkEventPort::subscribe_events`
//!   across libp2p swarm restarts).
//! * On every `PresenceEvent`, fetches a fresh peer snapshot via the
//!   existing `get_p2p_peers_snapshot` use case and emits one
//!   `peers.changed` payload. The snapshot already encodes per-peer
//!   reachability + device name, so consumers don't need a separate
//!   increment event to stay correct.
//! * `RecvError::Lagged` is logged and skipped — the next event will
//!   always re-publish the canonical snapshot, so a missed transition
//!   self-heals.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use uc_app::runtime::CoreRuntime;
use uc_app::usecases::CoreUseCases;
use uc_core::ports::PresencePort;
use uc_daemon_contract::constants::{ws_event, ws_topic};

use crate::api::projection::IntoApiDto;
use crate::api::types::{DaemonWsEvent, PeerSnapshotDto, PeersChangedFullPayload};
use crate::service::{DaemonService, ServiceHealth};

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn emit_ws_event<T: serde::Serialize>(
    event_tx: &broadcast::Sender<DaemonWsEvent>,
    topic: &str,
    event_type: &str,
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
        session_id: None,
        ts: now_ms(),
        payload,
    });
}

pub struct PresenceMonitor {
    presence: Arc<dyn PresencePort>,
    runtime: Arc<CoreRuntime>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl PresenceMonitor {
    pub fn new(
        presence: Arc<dyn PresencePort>,
        runtime: Arc<CoreRuntime>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
    ) -> Self {
        Self {
            presence,
            runtime,
            event_tx,
        }
    }

    async fn publish_snapshot(&self) {
        let usecases = CoreUseCases::new(self.runtime.as_ref());
        match usecases.get_p2p_peers_snapshot().execute().await {
            Ok(snapshots) => {
                let peers: Vec<PeerSnapshotDto> = snapshots
                    .into_iter()
                    .map(IntoApiDto::into_api_dto)
                    .collect();
                emit_ws_event(
                    &self.event_tx,
                    ws_topic::PEERS,
                    ws_event::PEERS_CHANGED,
                    PeersChangedFullPayload { peers },
                );
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "presence monitor: failed to fetch peer snapshot for peers.changed"
                );
            }
        }
    }
}

#[async_trait]
impl DaemonService for PresenceMonitor {
    fn name(&self) -> &str {
        "presence-monitor"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        info!("presence monitor starting");
        let mut rx = self.presence.subscribe();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    debug!("presence monitor cancelled");
                    return Ok(());
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            debug!(
                                device = %event.device_id.as_str(),
                                state = ?event.state,
                                "presence monitor: state change → publishing peers.changed"
                            );
                            self.publish_snapshot().await;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                skipped,
                                "presence monitor: dropped events (lagged); next event will re-publish snapshot"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            warn!("presence monitor: subscription closed; exiting loop");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}
