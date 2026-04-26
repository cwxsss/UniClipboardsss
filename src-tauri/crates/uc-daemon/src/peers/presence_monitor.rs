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
//!   so no reconnect/backoff loop is needed.
//! * On every `PresenceEvent`, fetches a fresh peer snapshot via the
//!   injected [`PeerSnapshotProvider`] and emits one `peers.changed`
//!   payload. Production now derives the snapshot from
//!   `MemberRepositoryPort` (membership 真相) + `PresencePort.current_state`
//!   (online/offline 当前态),不再依赖已退役的 libp2p `PeerDirectoryPort`。
//! * `RecvError::Lagged` is logged and skipped — the next event will
//!   always re-publish the canonical snapshot, so a missed transition
//!   self-heals.
//!
//! The snapshot fetch is abstracted behind `PeerSnapshotProvider` so the
//! loop logic can be unit-tested without constructing a full
//! [`CoreRuntime`]. Production wires `CoreRuntimePeerSnapshotProvider`,
//! tests inject a lightweight fake.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use uc_app::runtime::CoreRuntime;
use uc_core::ports::presence::ReachabilityState;
use uc_core::ports::PresencePort;
use uc_daemon_contract::constants::{ws_event, ws_topic};

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

/// Source of `PeerSnapshotDto` lists, abstracted so the monitor's loop
/// logic can be unit-tested without wiring a full `CoreRuntime`.
#[async_trait]
pub(crate) trait PeerSnapshotProvider: Send + Sync {
    async fn fetch(&self) -> anyhow::Result<Vec<PeerSnapshotDto>>;
}

/// Production provider — derives the snapshot from the member repository
/// (membership 真相) plus `PresencePort.current_state` per remote member
/// (online/offline 当前态)。
///
/// Local device 通过 `DeviceIdentityPort.current_device_id()` 排除。
/// 这里没有 "discovered but not yet a member" 概念——iroh stack 下,只有
/// 走完 pairing 进入 `space_member` 的设备才会出现在 peers.changed 里。
struct CoreRuntimePeerSnapshotProvider {
    presence: Arc<dyn PresencePort>,
    runtime: Arc<CoreRuntime>,
}

#[async_trait]
impl PeerSnapshotProvider for CoreRuntimePeerSnapshotProvider {
    async fn fetch(&self) -> anyhow::Result<Vec<PeerSnapshotDto>> {
        let deps = self.runtime.wiring_deps();
        let local_id = deps.device.device_identity.current_device_id();
        let members = deps
            .device
            .member_repo
            .list()
            .await
            .map_err(|e| anyhow::anyhow!("failed to list space members: {e}"))?;

        let mut snapshots = Vec::with_capacity(members.len());
        for member in members {
            if member.device_id == local_id {
                continue;
            }
            let state = self.presence.current_state(&member.device_id).await;
            let device_name = if member.device_name.is_empty() {
                None
            } else {
                Some(member.device_name.clone())
            };
            snapshots.push(PeerSnapshotDto {
                peer_id: member.device_id.as_str().to_string(),
                device_name,
                addresses: Vec::new(),
                is_paired: true,
                connected: matches!(state, ReachabilityState::Online),
                pairing_state: "Trusted".to_string(),
            });
        }
        Ok(snapshots)
    }
}

pub struct PresenceMonitor {
    presence: Arc<dyn PresencePort>,
    snapshot_provider: Arc<dyn PeerSnapshotProvider>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl PresenceMonitor {
    pub fn new(
        presence: Arc<dyn PresencePort>,
        runtime: Arc<CoreRuntime>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
    ) -> Self {
        let snapshot_provider: Arc<dyn PeerSnapshotProvider> =
            Arc::new(CoreRuntimePeerSnapshotProvider {
                presence: presence.clone(),
                runtime,
            });
        Self::with_snapshot_provider(presence, snapshot_provider, event_tx)
    }

    pub(crate) fn with_snapshot_provider(
        presence: Arc<dyn PresencePort>,
        snapshot_provider: Arc<dyn PeerSnapshotProvider>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
    ) -> Self {
        Self {
            presence,
            snapshot_provider,
            event_tx,
        }
    }

    async fn publish_snapshot(&self) {
        match self.snapshot_provider.fetch().await {
            Ok(peers) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;
    use std::time::Duration;

    use chrono::Utc;
    use tokio::time::timeout;

    use uc_core::ids::DeviceId;
    use uc_core::ports::presence::{PresenceError, PresenceEvent, ReachabilityState};

    /// Fake `PresencePort` whose `subscribe()` hands out receivers attached
    /// to a caller-controlled broadcast::Sender. The inner sender lives in
    /// a `Mutex<Option<_>>` so tests can `close()` the channel and verify
    /// the monitor exits on `RecvError::Closed`.
    struct FakePresence {
        tx: Mutex<Option<broadcast::Sender<PresenceEvent>>>,
    }

    impl FakePresence {
        fn new(capacity: usize) -> (Arc<Self>, broadcast::Sender<PresenceEvent>) {
            let (tx, _) = broadcast::channel(capacity);
            let port = Arc::new(Self {
                tx: Mutex::new(Some(tx.clone())),
            });
            (port, tx)
        }

        fn close(&self) {
            self.tx.lock().unwrap().take();
        }
    }

    #[async_trait]
    impl PresencePort for FakePresence {
        async fn ensure_reachable(
            &self,
            _device: &DeviceId,
        ) -> Result<ReachabilityState, PresenceError> {
            unreachable!("PresenceMonitor should not call ensure_reachable")
        }

        async fn current_state(&self, _device: &DeviceId) -> ReachabilityState {
            unreachable!("PresenceMonitor should not call current_state")
        }

        fn subscribe(&self) -> broadcast::Receiver<PresenceEvent> {
            self.tx
                .lock()
                .unwrap()
                .as_ref()
                .expect("FakePresence closed before subscribe")
                .subscribe()
        }
    }

    /// Fake snapshot provider — counts calls and either returns a canned
    /// list or a synthetic error.
    #[derive(Default)]
    struct FakeSnapshotProvider {
        inner: Mutex<FakeSnapshotInner>,
    }

    #[derive(Default)]
    struct FakeSnapshotInner {
        call_count: usize,
        succeed_with: Vec<PeerSnapshotDto>,
        fail: bool,
    }

    impl FakeSnapshotProvider {
        fn returning(peers: Vec<PeerSnapshotDto>) -> Arc<Self> {
            Arc::new(Self {
                inner: Mutex::new(FakeSnapshotInner {
                    call_count: 0,
                    succeed_with: peers,
                    fail: false,
                }),
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                inner: Mutex::new(FakeSnapshotInner {
                    call_count: 0,
                    succeed_with: Vec::new(),
                    fail: true,
                }),
            })
        }

        fn call_count(&self) -> usize {
            self.inner.lock().unwrap().call_count
        }
    }

    #[async_trait]
    impl PeerSnapshotProvider for FakeSnapshotProvider {
        async fn fetch(&self) -> anyhow::Result<Vec<PeerSnapshotDto>> {
            let mut guard = self.inner.lock().unwrap();
            guard.call_count += 1;
            if guard.fail {
                Err(anyhow::anyhow!("synthetic fetch failure"))
            } else {
                Ok(guard.succeed_with.clone())
            }
        }
    }

    fn online_event(device: &str) -> PresenceEvent {
        PresenceEvent {
            device_id: DeviceId::new(device),
            state: ReachabilityState::Online,
            at: Utc::now(),
        }
    }

    fn sample_dto(peer_id: &str) -> PeerSnapshotDto {
        PeerSnapshotDto {
            peer_id: peer_id.to_string(),
            device_name: Some("alpha".to_string()),
            addresses: vec![],
            is_paired: true,
            connected: true,
            pairing_state: "Trusted".to_string(),
        }
    }

    /// Wait for the next `peers.changed` event on the daemon broadcast,
    /// skipping any unrelated events. Returns `None` on timeout.
    async fn next_peers_changed(
        rx: &mut broadcast::Receiver<DaemonWsEvent>,
        wait: Duration,
    ) -> Option<DaemonWsEvent> {
        timeout(wait, async {
            loop {
                match rx.recv().await {
                    Ok(ev)
                        if ev.topic == ws_topic::PEERS
                            && ev.event_type == ws_event::PEERS_CHANGED =>
                    {
                        return ev;
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        unreachable!("ws event sender dropped before peers.changed arrived")
                    }
                }
            }
        })
        .await
        .ok()
    }

    #[tokio::test]
    async fn presence_event_publishes_peers_changed() {
        let (presence, presence_tx) = FakePresence::new(8);
        let snapshots = FakeSnapshotProvider::returning(vec![sample_dto("peer-a")]);
        let (event_tx, mut event_rx) = broadcast::channel::<DaemonWsEvent>(8);

        let monitor = Arc::new(PresenceMonitor::with_snapshot_provider(
            presence,
            snapshots.clone(),
            event_tx,
        ));
        let cancel = CancellationToken::new();
        let task = {
            let monitor = monitor.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { monitor.start(cancel).await })
        };

        // Give the monitor a chance to subscribe before we publish.
        tokio::task::yield_now().await;
        presence_tx.send(online_event("device-1")).unwrap();

        let ev = next_peers_changed(&mut event_rx, Duration::from_millis(500))
            .await
            .expect("peers.changed should be published");

        assert_eq!(ev.topic, ws_topic::PEERS);
        assert_eq!(ev.event_type, ws_event::PEERS_CHANGED);
        let peers = ev.payload.get("peers").and_then(|v| v.as_array()).unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(
            peers[0].get("peerId").and_then(|v| v.as_str()),
            Some("peer-a")
        );
        assert_eq!(snapshots.call_count(), 1);

        cancel.cancel();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn each_event_triggers_independent_snapshot() {
        let (presence, presence_tx) = FakePresence::new(8);
        let snapshots = FakeSnapshotProvider::returning(vec![sample_dto("peer-a")]);
        let (event_tx, mut event_rx) = broadcast::channel::<DaemonWsEvent>(8);

        let monitor = Arc::new(PresenceMonitor::with_snapshot_provider(
            presence,
            snapshots.clone(),
            event_tx,
        ));
        let cancel = CancellationToken::new();
        let task = {
            let monitor = monitor.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { monitor.start(cancel).await })
        };

        tokio::task::yield_now().await;
        presence_tx.send(online_event("device-1")).unwrap();
        next_peers_changed(&mut event_rx, Duration::from_millis(500))
            .await
            .expect("first peers.changed");

        presence_tx.send(online_event("device-2")).unwrap();
        next_peers_changed(&mut event_rx, Duration::from_millis(500))
            .await
            .expect("second peers.changed");

        assert_eq!(snapshots.call_count(), 2);

        cancel.cancel();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn snapshot_failure_does_not_exit_loop() {
        let (presence, presence_tx) = FakePresence::new(8);
        let snapshots = FakeSnapshotProvider::failing();
        let (event_tx, mut event_rx) = broadcast::channel::<DaemonWsEvent>(8);

        let monitor = Arc::new(PresenceMonitor::with_snapshot_provider(
            presence,
            snapshots.clone(),
            event_tx,
        ));
        let cancel = CancellationToken::new();
        let task = {
            let monitor = monitor.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { monitor.start(cancel).await })
        };

        tokio::task::yield_now().await;
        presence_tx.send(online_event("device-1")).unwrap();
        // Failure path: no peers.changed should be emitted.
        assert!(
            next_peers_changed(&mut event_rx, Duration::from_millis(150))
                .await
                .is_none(),
            "peers.changed should not be emitted on snapshot failure"
        );
        assert_eq!(snapshots.call_count(), 1);

        // Loop must still be alive — a second event also triggers a fetch.
        presence_tx.send(online_event("device-2")).unwrap();
        // Allow the monitor to drain the second event before sampling.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(snapshots.call_count(), 2);

        cancel.cancel();
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn cancellation_exits_loop() {
        let (presence, _presence_tx) = FakePresence::new(8);
        let snapshots = FakeSnapshotProvider::returning(vec![]);
        let (event_tx, _event_rx) = broadcast::channel::<DaemonWsEvent>(8);

        let monitor = Arc::new(PresenceMonitor::with_snapshot_provider(
            presence, snapshots, event_tx,
        ));
        let cancel = CancellationToken::new();
        let task = {
            let monitor = monitor.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { monitor.start(cancel).await })
        };

        cancel.cancel();
        timeout(Duration::from_millis(500), task)
            .await
            .expect("monitor should exit promptly after cancel")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn closed_subscription_exits_loop() {
        let (presence, presence_tx) = FakePresence::new(8);
        let snapshots = FakeSnapshotProvider::returning(vec![]);
        let (event_tx, _event_rx) = broadcast::channel::<DaemonWsEvent>(8);

        let monitor = Arc::new(PresenceMonitor::with_snapshot_provider(
            presence.clone(),
            snapshots,
            event_tx,
        ));
        let cancel = CancellationToken::new();
        let task = {
            let monitor = monitor.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { monitor.start(cancel).await })
        };

        // Let the monitor subscribe, then drop both sender handles
        // (external + the one held inside FakePresence) so the
        // subscription returns RecvError::Closed.
        tokio::task::yield_now().await;
        drop(presence_tx);
        presence.close();

        timeout(Duration::from_millis(500), task)
            .await
            .expect("monitor should exit promptly after sender drop")
            .unwrap()
            .unwrap();
    }
}
