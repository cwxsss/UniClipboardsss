use std::sync::Arc;

use tokio::sync::mpsc;
use uc_core::ports::host_event_emitter::{SetupHostEvent, SpaceAccessHostEvent};
use uc_core::ports::{
    HostEvent, HostEventEmitterPort, RealtimeEvent, RealtimeTopic, RealtimeTopicPort,
};

pub async fn run_setup_state_realtime_consumer(
    realtime: Arc<dyn RealtimeTopicPort>,
    emitter: Arc<dyn HostEventEmitterPort>,
) -> anyhow::Result<()> {
    let mut rx = realtime
        .subscribe("setup_state_realtime_consumer", &[RealtimeTopic::Setup])
        .await?;

    run_setup_state_realtime_consumer_with_rx(&mut rx, emitter).await
}

pub async fn run_setup_state_realtime_consumer_with_rx(
    rx: &mut mpsc::Receiver<RealtimeEvent>,
    emitter: Arc<dyn HostEventEmitterPort>,
) -> anyhow::Result<()> {
    while let Some(event) = rx.recv().await {
        match event {
            RealtimeEvent::SetupStateChanged(payload) => {
                let _ = emitter.emit(HostEvent::Setup(SetupHostEvent::StateChanged {
                    state: payload.state,
                    session_id: payload.session_id,
                }));
            }
            RealtimeEvent::SetupSpaceAccessCompleted(payload) => {
                let _ = emitter.emit(HostEvent::SpaceAccess(SpaceAccessHostEvent::Completed {
                    session_id: payload.session_id,
                    peer_id: payload.peer_id,
                    success: payload.success,
                    reason: payload.reason,
                    ts: payload.ts,
                }));
            }
            _ => {}
        }
    }

    Ok(())
}
