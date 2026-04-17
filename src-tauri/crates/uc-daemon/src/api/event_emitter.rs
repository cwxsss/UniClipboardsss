use serde::Serialize;
use tokio::sync::broadcast;
use uc_app::shared::host_event::{
    ClipboardHostEvent, EmitError, HostEvent, HostEventEmitterPort, SetupHostEvent,
    SpaceAccessHostEvent, TransferHostEvent,
};
use uc_daemon_contract::constants::{ws_event, ws_topic};

use crate::api::types::{
    DaemonWsEvent, FileTransferProgressPayload, SetupSpaceAccessCompletedPayload,
    SetupStateChangedPayload,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileTransferStatusChangedPayload {
    transfer_id: String,
    entry_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardNewContentPayload {
    entry_id: String,
    preview: String,
    origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
}

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
        topic: &str,
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
            topic: topic.to_string(),
            event_type: event_type.to_string(),
            session_id,
            ts,
            payload,
        });
    }
}

impl HostEventEmitterPort for DaemonApiEventEmitter {
    fn emit(&self, event: HostEvent) -> Result<(), EmitError> {
        match event {
            HostEvent::Setup(SetupHostEvent::StateChanged { state, session_id }) => {
                self.emit_ws_event(
                    ws_event::SETUP_STATE_CHANGED,
                    ws_topic::SETUP,
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
                    ws_event::SETUP_SPACE_ACCESS_COMPLETED,
                    ws_topic::SETUP,
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
            HostEvent::Transfer(TransferHostEvent::StatusChanged {
                transfer_id,
                entry_id,
                status,
                reason,
            }) => {
                self.emit_ws_event(
                    ws_event::FILE_TRANSFER_STATUS_CHANGED,
                    ws_topic::FILE_TRANSFER,
                    None,
                    Self::now_ms(),
                    FileTransferStatusChangedPayload {
                        transfer_id,
                        entry_id,
                        status,
                        reason,
                    },
                );
            }
            HostEvent::Transfer(TransferHostEvent::Progress {
                transfer_id,
                entry_id,
                peer_id,
                direction,
                bytes_transferred,
                total_bytes,
            }) => {
                self.emit_ws_event(
                    ws_event::FILE_TRANSFER_PROGRESS,
                    ws_topic::FILE_TRANSFER,
                    None,
                    Self::now_ms(),
                    FileTransferProgressPayload {
                        transfer_id,
                        entry_id,
                        peer_id,
                        direction,
                        bytes_transferred,
                        total_bytes,
                    },
                );
            }
            HostEvent::Clipboard(ClipboardHostEvent::NewContent {
                entry_id,
                preview,
                origin,
            }) => {
                self.emit_ws_event(
                    ws_event::CLIPBOARD_NEW_CONTENT,
                    ws_topic::CLIPBOARD,
                    None,
                    Self::now_ms(),
                    ClipboardNewContentPayload {
                        entry_id,
                        preview,
                        origin: format!("{:?}", origin).to_lowercase(),
                        content_type: None,
                    },
                );
            }
        }

        Ok(())
    }
}
