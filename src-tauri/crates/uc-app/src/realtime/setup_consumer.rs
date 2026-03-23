use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};
use uc_core::network::pairing_state_machine::FailureReason;
use uc_core::ports::{
    PairingVerificationRequiredEvent, RealtimeEvent, RealtimeTopic, RealtimeTopicPort,
};

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

    run_setup_realtime_consumer_with_rx(&mut rx, hub).await
}

pub async fn run_setup_realtime_consumer_with_rx(
    rx: &mut mpsc::Receiver<RealtimeEvent>,
    hub: Arc<SetupPairingEventHub>,
) -> anyhow::Result<()> {
    let mut session_stages = HashMap::new();

    while let Some(event) = rx.recv().await {
        if let Some((session_id, stage, domain_event)) = map_setup_event(event.clone()) {
            if should_forward(&session_stages, &session_id, stage) {
                info!(
                    session_id = %session_id,
                    stage = ?stage,
                    event = ?domain_event,
                    "forwarding setup pairing event from realtime consumer"
                );
                hub.publish(domain_event).await?;
                session_stages.insert(session_id, stage);
            } else {
                debug!(
                    session_id = %session_id,
                    current_stage = ?session_stages.get(&session_id),
                    next_stage = ?stage,
                    "dropping out-of-order setup pairing event"
                );
            }
        } else if let Some((session_id, reason)) = map_setup_event_rejection(&event) {
            warn!(
                session_id = %session_id,
                rejection_reason = %reason,
                event = ?event,
                "setup realtime consumer rejected pairing event"
            );
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SetupSessionStage {
    VerificationRequired,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerificationPayloadRejection {
    PeerIdMissing,
    CodeMissing,
    LocalFingerprintMissing,
    PeerFingerprintMissing,
}

impl VerificationPayloadRejection {
    fn as_str(self) -> &'static str {
        match self {
            Self::PeerIdMissing => "peer_id_missing",
            Self::CodeMissing => "code_missing",
            Self::LocalFingerprintMissing => "local_fingerprint_missing",
            Self::PeerFingerprintMissing => "peer_fingerprint_missing",
        }
    }
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
            let session_id = payload.session_id.clone();
            let Ok((peer_id, short_code, local_fingerprint, peer_fingerprint)) =
                validate_verification_payload(payload)
            else {
                return None;
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

fn map_setup_event_rejection(event: &RealtimeEvent) -> Option<(String, String)> {
    match event {
        RealtimeEvent::PairingVerificationRequired(payload) => {
            validate_verification_payload(payload.clone())
                .err()
                .map(|reason| (payload.session_id.clone(), reason.as_str().to_string()))
        }
        _ => None,
    }
}

fn validate_verification_payload(
    payload: PairingVerificationRequiredEvent,
) -> Result<(String, String, String, String), VerificationPayloadRejection> {
    let peer_id = payload
        .peer_id
        .ok_or(VerificationPayloadRejection::PeerIdMissing)?;
    let short_code = payload
        .code
        .ok_or(VerificationPayloadRejection::CodeMissing)?;
    let local_fingerprint = payload
        .local_fingerprint
        .ok_or(VerificationPayloadRejection::LocalFingerprintMissing)?;
    let peer_fingerprint = payload
        .peer_fingerprint
        .ok_or(VerificationPayloadRejection::PeerFingerprintMissing)?;

    Ok((peer_id, short_code, local_fingerprint, peer_fingerprint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_setup_event_reports_missing_peer_id_for_verification_payload() {
        let event = RealtimeEvent::PairingVerificationRequired(PairingVerificationRequiredEvent {
            session_id: "session-missing-peer".into(),
            peer_id: None,
            device_name: Some("Desk".into()),
            code: Some("123456".into()),
            local_fingerprint: Some("local".into()),
            peer_fingerprint: Some("peer".into()),
        });

        let rejection = map_setup_event_rejection(&event);

        assert_eq!(
            rejection,
            Some((
                "session-missing-peer".to_string(),
                "peer_id_missing".to_string()
            ))
        );
    }
}
