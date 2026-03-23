use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::security::space_access::state::SpaceAccessState;
use crate::setup::SetupState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RealtimeTopic {
    Pairing,
    Peers,
    PairedDevices,
    Setup,
    SpaceAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingUpdatedEvent {
    pub session_id: String,
    pub status: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingVerificationRequiredEvent {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
    pub code: Option<String>,
    pub local_fingerprint: Option<String>,
    pub peer_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingFailedEvent {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCompleteEvent {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimePeerSummary {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerChangedEvent {
    pub peers: Vec<RealtimePeerSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerNameUpdatedEvent {
    pub peer_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerConnectionChangedEvent {
    pub peer_id: String,
    pub connected: bool,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimePairedDeviceSummary {
    pub device_id: String,
    pub device_name: String,
    pub last_seen_ts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDevicesChangedEvent {
    pub devices: Vec<RealtimePairedDeviceSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetupStateChangedEvent {
    pub session_id: Option<String>,
    pub state: SetupState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupSpaceAccessCompletedEvent {
    pub session_id: String,
    pub peer_id: String,
    pub success: bool,
    pub reason: Option<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpaceAccessStateChangedEvent {
    pub state: SpaceAccessState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RealtimeEvent {
    PairingUpdated(PairingUpdatedEvent),
    PairingVerificationRequired(PairingVerificationRequiredEvent),
    PairingFailed(PairingFailedEvent),
    PairingComplete(PairingCompleteEvent),
    PeersChanged(PeerChangedEvent),
    PeersNameUpdated(PeerNameUpdatedEvent),
    PeersConnectionChanged(PeerConnectionChangedEvent),
    PairedDevicesChanged(PairedDevicesChangedEvent),
    SetupStateChanged(SetupStateChangedEvent),
    SetupSpaceAccessCompleted(SetupSpaceAccessCompletedEvent),
    SpaceAccessStateChanged(SpaceAccessStateChangedEvent),
}

pub const FRONTEND_REALTIME_EVENT: &str = "daemon://realtime";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeFrontendPayload {
    PairingUpdated(PairingUpdatedEvent),
    PairingVerificationRequired(PairingVerificationRequiredEvent),
    PairingFailed(PairingFailedEvent),
    PairingComplete(PairingCompleteEvent),
    PeersChanged(PeerChangedEvent),
    PeersNameUpdated(PeerNameUpdatedEvent),
    PeersConnectionChanged(PeerConnectionChangedEvent),
    PairedDevicesChanged(PairedDevicesChangedEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimeFrontendEvent {
    pub topic: RealtimeTopic,
    pub r#type: &'static str,
    pub ts: i64,
    pub payload: RealtimeFrontendPayload,
}

impl RealtimeFrontendEvent {
    pub fn new(
        topic: RealtimeTopic,
        event_type: &'static str,
        ts: i64,
        payload: RealtimeFrontendPayload,
    ) -> Self {
        Self {
            topic,
            r#type: event_type,
            ts,
            payload,
        }
    }

    pub fn event_type(&self) -> &'static str {
        self.r#type
    }
}

#[async_trait]
pub trait RealtimeTopicPort: Send + Sync {
    async fn subscribe(
        &self,
        consumer: &'static str,
        topics: &[RealtimeTopic],
    ) -> anyhow::Result<mpsc::Receiver<RealtimeEvent>>;
}
