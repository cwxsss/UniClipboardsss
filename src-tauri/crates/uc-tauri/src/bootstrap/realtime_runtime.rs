use std::sync::Arc;

use tracing::warn;
use uc_app::realtime::{
    run_pairing_realtime_consumer, run_peers_realtime_consumer, run_setup_realtime_consumer,
    SetupPairingEventHub,
};
use uc_app::task_registry::TaskRegistry;
use uc_core::ports::HostEventEmitterPort;

use super::daemon_ws_bridge::DaemonWsBridgeConfig;
use super::{DaemonConnectionState, DaemonWsBridge};

pub async fn start_realtime_runtime(
    connection_state: DaemonConnectionState,
    event_emitter: Arc<dyn HostEventEmitterPort>,
    task_registry: &Arc<TaskRegistry>,
) {
    let bridge = Arc::new(DaemonWsBridge::new(
        connection_state,
        DaemonWsBridgeConfig::default(),
    ));
    let setup_hub = Arc::new(SetupPairingEventHub::new(64));

    let pairing_bridge = bridge.clone();
    let pairing_emitter = event_emitter.clone();
    task_registry
        .spawn("realtime_pairing_consumer", |_token| async move {
            if let Err(err) = run_pairing_realtime_consumer(pairing_bridge, pairing_emitter).await {
                warn!(error = %err, "pairing realtime consumer stopped");
            }
        })
        .await;

    let peers_bridge = bridge.clone();
    let peers_emitter = event_emitter.clone();
    task_registry
        .spawn("realtime_peers_consumer", |_token| async move {
            if let Err(err) = run_peers_realtime_consumer(peers_bridge, peers_emitter).await {
                warn!(error = %err, "peers realtime consumer stopped");
            }
        })
        .await;

    let setup_bridge = bridge.clone();
    let setup_hub_clone = setup_hub.clone();
    task_registry
        .spawn("realtime_setup_consumer", |_token| async move {
            if let Err(err) = run_setup_realtime_consumer(setup_bridge, setup_hub_clone).await {
                warn!(error = %err, "setup realtime consumer stopped");
            }
        })
        .await;

    task_registry
        .spawn("daemon_ws_bridge", move |token| async move {
            if let Err(err) = bridge.run(token).await {
                warn!(error = %err, "daemon websocket bridge stopped");
            }
        })
        .await;
}
