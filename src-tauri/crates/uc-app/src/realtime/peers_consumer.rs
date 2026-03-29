use std::sync::Arc;

use tokio::sync::mpsc;
use uc_core::ports::{
    HostEvent, HostEventEmitterPort, RealtimeEvent, RealtimeFrontendEvent, RealtimeFrontendPayload,
    RealtimeTopic, RealtimeTopicPort,
};

pub async fn run_peers_realtime_consumer(
    realtime: Arc<dyn RealtimeTopicPort>,
    emitter: Arc<dyn HostEventEmitterPort>,
) -> anyhow::Result<()> {
    let mut rx = realtime
        .subscribe("peers_realtime_consumer", &[RealtimeTopic::Peers])
        .await?;

    run_peers_realtime_consumer_with_rx(&mut rx, emitter).await
}

pub async fn run_peers_realtime_consumer_with_rx(
    rx: &mut mpsc::Receiver<RealtimeEvent>,
    emitter: Arc<dyn HostEventEmitterPort>,
) -> anyhow::Result<()> {
    while let Some(event) = rx.recv().await {
        if let Some(event) = map_peers_event(event) {
            let _ = emitter.emit(HostEvent::Realtime(event));
        }
    }

    Ok(())
}

fn map_peers_event(event: RealtimeEvent) -> Option<RealtimeFrontendEvent> {
    let (event_type, payload) = match event {
        RealtimeEvent::PeersChanged(payload) => (
            "peers.changed",
            RealtimeFrontendPayload::PeersChanged(payload),
        ),
        RealtimeEvent::PeersNameUpdated(payload) => (
            "peers.nameUpdated",
            RealtimeFrontendPayload::PeersNameUpdated(payload),
        ),
        RealtimeEvent::PeersConnectionChanged(payload) => (
            "peers.connectionChanged",
            RealtimeFrontendPayload::PeersConnectionChanged(payload),
        ),
        _ => return None,
    };

    Some(RealtimeFrontendEvent::new(
        RealtimeTopic::Peers,
        event_type,
        0,
        payload,
    ))
}
