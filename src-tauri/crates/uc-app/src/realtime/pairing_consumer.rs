use std::sync::Arc;

use tokio::sync::mpsc;
use uc_core::ports::{
    HostEvent, HostEventEmitterPort, RealtimeEvent, RealtimeFrontendEvent, RealtimeFrontendPayload,
    RealtimeTopic, RealtimeTopicPort,
};

pub async fn run_pairing_realtime_consumer(
    realtime: Arc<dyn RealtimeTopicPort>,
    emitter: Arc<dyn HostEventEmitterPort>,
) -> anyhow::Result<()> {
    let mut rx = realtime
        .subscribe("pairing_realtime_consumer", &[RealtimeTopic::Pairing])
        .await?;

    run_pairing_realtime_consumer_with_rx(&mut rx, emitter).await
}

pub async fn run_pairing_realtime_consumer_with_rx(
    rx: &mut mpsc::Receiver<RealtimeEvent>,
    emitter: Arc<dyn HostEventEmitterPort>,
) -> anyhow::Result<()> {
    while let Some(event) = rx.recv().await {
        if let Some(event) = map_pairing_event(event) {
            let _ = emitter.emit(HostEvent::Realtime(event));
        }
    }

    Ok(())
}

fn map_pairing_event(event: RealtimeEvent) -> Option<RealtimeFrontendEvent> {
    let (event_type, payload) = match event {
        RealtimeEvent::PairingUpdated(payload) => (
            "pairing.updated",
            RealtimeFrontendPayload::PairingUpdated(payload),
        ),
        RealtimeEvent::PairingVerificationRequired(payload) => (
            "pairing.verificationRequired",
            RealtimeFrontendPayload::PairingVerificationRequired(payload),
        ),
        RealtimeEvent::PairingFailed(payload) => (
            "pairing.failed",
            RealtimeFrontendPayload::PairingFailed(payload),
        ),
        RealtimeEvent::PairingComplete(payload) => (
            "pairing.complete",
            RealtimeFrontendPayload::PairingComplete(payload),
        ),
        _ => return None,
    };

    Some(RealtimeFrontendEvent::new(
        RealtimeTopic::Pairing,
        event_type,
        0,
        payload,
    ))
}
