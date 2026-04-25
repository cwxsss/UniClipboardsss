//! Daemon-side broadcaster for the new stateless setup pairing events
//! (Slice4 Phase 3 T3.1).
//!
//! T3.3 will inject this into `SpaceSetupFacade` so its lifecycle
//! callbacks can fan out to the `setup` ws topic with a single call.
//! The legacy `setup.stateChanged` / `setup.spaceAccessCompleted` path
//! stays in `event_emitter.rs` until T3.4 deletes the old setup module.

use serde::Serialize;
use tokio::sync::broadcast;
use uc_daemon_contract::api::dto::setup_events::{
    SetupInvitationIssuedEvent, SetupInvitationRevokedEvent, SetupPairingCompletedEvent,
};
use uc_daemon_contract::api::types::DaemonWsEvent;
use uc_daemon_contract::constants::{ws_event, ws_topic};

/// Fans out setup pairing lifecycle events onto the daemon ws bus
/// under `ws_topic::SETUP`.
#[derive(Clone)]
pub struct SetupEventBroadcaster {
    event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl SetupEventBroadcaster {
    pub fn new(event_tx: broadcast::Sender<DaemonWsEvent>) -> Self {
        Self { event_tx }
    }

    pub fn emit_invitation_issued(&self, code: String, expires_at_ms: i64) {
        self.send(
            ws_event::SETUP_INVITATION_ISSUED,
            SetupInvitationIssuedEvent {
                code,
                expires_at_ms,
            },
        );
    }

    pub fn emit_pairing_completed(
        &self,
        sponsor_device_id: String,
        joiner_device_id: String,
        success: bool,
        reason: Option<String>,
    ) {
        self.send(
            ws_event::SETUP_PAIRING_COMPLETED,
            SetupPairingCompletedEvent {
                sponsor_device_id,
                joiner_device_id,
                success,
                reason,
            },
        );
    }

    pub fn emit_invitation_revoked(&self, reason: String) {
        self.send(
            ws_event::SETUP_INVITATION_REVOKED,
            SetupInvitationRevokedEvent { reason },
        );
    }

    fn send<T: Serialize>(&self, event_type: &'static str, payload: T) {
        let payload = match serde_json::to_value(payload) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(error = %error, event_type, "failed to serialize setup ws event");
                return;
            }
        };

        let _ = self.event_tx.send(DaemonWsEvent {
            topic: ws_topic::SETUP.to_string(),
            event_type: event_type.to_string(),
            session_id: None,
            ts: chrono::Utc::now().timestamp_millis(),
            payload,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    fn setup() -> (SetupEventBroadcaster, broadcast::Receiver<DaemonWsEvent>) {
        let (tx, rx) = broadcast::channel(8);
        (SetupEventBroadcaster::new(tx), rx)
    }

    #[tokio::test]
    async fn invitation_issued_routes_to_setup_topic_with_camel_case_payload() {
        let (broadcaster, mut rx) = setup();

        broadcaster.emit_invitation_issued("ABCD-1234".to_string(), 1_745_577_600_000);
        let event = rx.recv().await.expect("event delivered");

        assert_eq!(event.topic, "setup");
        assert_eq!(event.event_type, "setup.invitationIssued");
        assert!(event.session_id.is_none());
        assert_eq!(event.payload["code"], "ABCD-1234");
        assert_eq!(event.payload["expiresAtMs"], 1_745_577_600_000_i64);
        assert!(event.payload.get("expires_at_ms").is_none());
    }

    #[tokio::test]
    async fn pairing_completed_carries_both_device_ids_and_reason() {
        let (broadcaster, mut rx) = setup();

        broadcaster.emit_pairing_completed(
            "sponsor-1".to_string(),
            "joiner-2".to_string(),
            false,
            Some("timeout".to_string()),
        );
        let event = rx.recv().await.expect("event delivered");

        assert_eq!(event.topic, "setup");
        assert_eq!(event.event_type, "setup.pairingCompleted");
        assert_eq!(event.payload["sponsorDeviceId"], "sponsor-1");
        assert_eq!(event.payload["joinerDeviceId"], "joiner-2");
        assert_eq!(event.payload["success"], false);
        assert_eq!(event.payload["reason"], "timeout");
    }

    #[tokio::test]
    async fn pairing_completed_success_carries_null_reason() {
        let (broadcaster, mut rx) = setup();

        broadcaster.emit_pairing_completed(
            "sponsor-1".to_string(),
            "joiner-2".to_string(),
            true,
            None,
        );
        let event = rx.recv().await.expect("event delivered");

        assert_eq!(event.payload["success"], true);
        assert!(event.payload["reason"].is_null());
    }

    #[tokio::test]
    async fn invitation_revoked_carries_reason() {
        let (broadcaster, mut rx) = setup();

        broadcaster.emit_invitation_revoked("cancelled".to_string());
        let event = rx.recv().await.expect("event delivered");

        assert_eq!(event.topic, "setup");
        assert_eq!(event.event_type, "setup.invitationRevoked");
        assert_eq!(event.payload["reason"], "cancelled");
    }

    #[tokio::test]
    async fn ts_is_populated_with_recent_wallclock() {
        let (broadcaster, mut rx) = setup();
        let before = chrono::Utc::now().timestamp_millis();

        broadcaster.emit_invitation_revoked("expired".to_string());
        let event = rx.recv().await.expect("event delivered");
        let after = chrono::Utc::now().timestamp_millis();

        assert!(
            event.ts >= before && event.ts <= after,
            "ts={} not in [{}, {}]",
            event.ts,
            before,
            after
        );
    }
}
