use tokio::sync::broadcast;
use uc_core::ports::host_event_emitter::{
    EmitError, HostEvent, HostEventEmitterPort, SetupHostEvent, SpaceAccessHostEvent,
};

use crate::api::types::{
    DaemonWsEvent, SetupSpaceAccessCompletedPayload, SetupStateChangedPayload,
};

const TOPIC_SETUP: &str = "setup";
const SETUP_STATE_CHANGED_EVENT: &str = "setup.state_changed";
const SETUP_SPACE_ACCESS_COMPLETED_EVENT: &str = "setup.space_access_completed";

pub struct DaemonApiEventEmitter {
    event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl DaemonApiEventEmitter {
    pub fn new(event_tx: broadcast::Sender<DaemonWsEvent>) -> Self {
        Self { event_tx }
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn emit_ws_event<T: serde::Serialize>(
        &self,
        event_type: &str,
        session_id: Option<String>,
        ts: i64,
        payload: T,
    ) {
        let payload = match serde_json::to_value(payload) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(error = %error, event_type, "failed to serialize daemon api event");
                return;
            }
        };

        let _ = self.event_tx.send(DaemonWsEvent {
            topic: TOPIC_SETUP.to_string(),
            event_type: event_type.to_string(),
            session_id,
            ts,
            payload,
        });
    }

    fn log_non_setup_event(event_type: &'static str) {
        tracing::debug!(event_type, "host event (daemon api emitter)");
    }
}

impl HostEventEmitterPort for DaemonApiEventEmitter {
    fn emit(&self, event: HostEvent) -> Result<(), EmitError> {
        match event {
            HostEvent::Setup(SetupHostEvent::StateChanged { state, session_id }) => {
                self.emit_ws_event(
                    SETUP_STATE_CHANGED_EVENT,
                    session_id.clone(),
                    Self::now_ms(),
                    SetupStateChangedPayload {
                        session_id,
                        state: serde_json::to_value(state).unwrap_or_default(),
                    },
                );
            }
            HostEvent::SpaceAccess(SpaceAccessHostEvent::Completed {
                session_id,
                peer_id,
                success,
                reason,
                ts,
            })
            | HostEvent::SpaceAccess(SpaceAccessHostEvent::P2PCompleted {
                session_id,
                peer_id,
                success,
                reason,
                ts,
            }) => {
                self.emit_ws_event(
                    SETUP_SPACE_ACCESS_COMPLETED_EVENT,
                    Some(session_id.clone()),
                    ts,
                    SetupSpaceAccessCompletedPayload {
                        session_id,
                        peer_id,
                        success,
                        reason,
                        ts,
                    },
                );
            }
            HostEvent::Clipboard(_) => Self::log_non_setup_event("clipboard"),
            HostEvent::PeerDiscovery(_) => Self::log_non_setup_event("peer_discovery"),
            HostEvent::PeerConnection(_) => Self::log_non_setup_event("peer_connection"),
            HostEvent::Transfer(_) => Self::log_non_setup_event("transfer"),
            HostEvent::Pairing(_) => Self::log_non_setup_event("pairing"),
            HostEvent::Realtime(_) => Self::log_non_setup_event("realtime"),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uc_core::ports::host_event_emitter::HostEvent;
    use uc_core::setup::SetupState;

    #[test]
    fn emits_setup_state_changed_to_setup_topic() {
        let (tx, mut rx) = broadcast::channel(4);
        let emitter = DaemonApiEventEmitter::new(tx);

        emitter
            .emit(HostEvent::Setup(SetupHostEvent::StateChanged {
                state: SetupState::JoinSpaceConfirmPeer {
                    short_code: "123456".to_string(),
                    peer_fingerprint: Some("peer-fp".to_string()),
                    error: None,
                },
                session_id: Some("session-1".to_string()),
            }))
            .expect("daemon api emitter should stay infallible");

        let event = rx.try_recv().expect("setup event should be broadcast");
        assert_eq!(event.topic, TOPIC_SETUP);
        assert_eq!(event.event_type, SETUP_STATE_CHANGED_EVENT);
        assert_eq!(event.session_id.as_deref(), Some("session-1"));
        assert_eq!(
            event.payload["sessionId"].as_str(),
            Some("session-1"),
            "setup payload should use camelCase sessionId"
        );
        assert!(
            event.payload["state"]["JoinSpaceConfirmPeer"].is_object(),
            "setup payload should carry full setup state object"
        );
    }
}
