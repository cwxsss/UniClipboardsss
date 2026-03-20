use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tracing::warn;
use uc_core::network::pairing_state_machine::FailureReason;
use uc_core::ports::{RealtimeEvent, RealtimeTopic, RealtimeTopicPort};

use crate::usecases::pairing::PairingDomainEvent;

pub struct SetupPairingEventHub {
    buffer: usize,
    subscribers: Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>,
}

impl SetupPairingEventHub {
    pub fn new(buffer: usize) -> Self {
        Self {
            buffer,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PairingDomainEvent>> {
        let (tx, rx) = mpsc::channel(self.buffer);
        self.subscribers.lock().await.push(tx);
        Ok(rx)
    }

    pub async fn publish(&self, event: PairingDomainEvent) -> anyhow::Result<()> {
        let subscribers = self.subscribers.lock().await.clone();
        let mut active = Vec::with_capacity(subscribers.len());

        for subscriber in subscribers {
            if subscriber.send(event.clone()).await.is_ok() {
                active.push(subscriber);
            }
        }

        *self.subscribers.lock().await = active;
        Ok(())
    }
}

pub async fn run_setup_realtime_consumer(
    realtime: Arc<dyn RealtimeTopicPort>,
    hub: Arc<SetupPairingEventHub>,
) -> anyhow::Result<()> {
    let mut rx = realtime
        .subscribe("setup_realtime_consumer", &[RealtimeTopic::Pairing])
        .await?;
    let mut session_stages = HashMap::new();

    while let Some(event) = rx.recv().await {
        if let Some((session_id, stage, domain_event)) = map_setup_event(event) {
            if should_forward(&session_stages, &session_id, stage) {
                hub.publish(domain_event).await?;
                session_stages.insert(session_id, stage);
            }
        }
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SetupSessionStage {
    VerificationRequired,
    Terminal,
}

fn should_forward(
    session_stages: &HashMap<String, SetupSessionStage>,
    session_id: &str,
    next_stage: SetupSessionStage,
) -> bool {
    match session_stages.get(session_id).copied() {
        Some(current_stage) => next_stage > current_stage,
        None => true,
    }
}

fn map_setup_event(
    event: RealtimeEvent,
) -> Option<(String, SetupSessionStage, PairingDomainEvent)> {
    match event {
        RealtimeEvent::PairingVerificationRequired(payload) => {
            let session_id = payload.session_id;
            let peer_id = match payload.peer_id {
                Some(peer_id) => peer_id,
                None => {
                    warn!(session_id = %session_id, "dropping setup pairing event without peer_id");
                    return None;
                }
            };
            let short_code = match payload.code {
                Some(short_code) => short_code,
                None => {
                    warn!(session_id = %session_id, "dropping setup pairing event without code");
                    return None;
                }
            };
            let local_fingerprint = match payload.local_fingerprint {
                Some(local_fingerprint) => local_fingerprint,
                None => {
                    warn!(session_id = %session_id, "dropping setup pairing event without local_fingerprint");
                    return None;
                }
            };
            let peer_fingerprint = match payload.peer_fingerprint {
                Some(peer_fingerprint) => peer_fingerprint,
                None => {
                    warn!(session_id = %session_id, "dropping setup pairing event without peer_fingerprint");
                    return None;
                }
            };

            let domain_event = PairingDomainEvent::PairingVerificationRequired {
                session_id: session_id.clone(),
                peer_id,
                short_code,
                local_fingerprint,
                peer_fingerprint,
            };

            Some((
                session_id,
                SetupSessionStage::VerificationRequired,
                domain_event,
            ))
        }
        RealtimeEvent::PairingComplete(payload) => {
            let session_id = payload.session_id;
            let peer_id = payload.peer_id.unwrap_or_default();
            let domain_event = PairingDomainEvent::PairingSucceeded {
                session_id: session_id.clone(),
                peer_id,
            };

            Some((session_id, SetupSessionStage::Terminal, domain_event))
        }
        RealtimeEvent::PairingFailed(payload) => {
            let session_id = payload.session_id;
            let domain_event = PairingDomainEvent::PairingFailed {
                session_id: session_id.clone(),
                peer_id: String::new(),
                reason: FailureReason::Other(payload.reason),
            };

            Some((session_id, SetupSessionStage::Terminal, domain_event))
        }
        _ => None,
    }
}
