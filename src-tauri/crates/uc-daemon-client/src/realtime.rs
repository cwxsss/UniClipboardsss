use async_trait::async_trait;
use tokio::sync::mpsc;

use uc_core::space_access::state::SpaceAccessState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RealtimeTopic {
    Pairing,
    Peers,
    PairedDevices,
    Setup,
    SpaceAccess,
    Clipboard,
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
pub struct RealtimeSpaceMemberSummary {
    pub device_id: String,
    pub device_name: String,
    pub last_seen_ts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceMembersChangedEvent {
    pub devices: Vec<RealtimeSpaceMemberSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpaceAccessStateChangedEvent {
    pub state: SpaceAccessState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardNewContentEvent {
    pub entry_id: String,
    pub preview: String,
    pub origin: String, // "local" or "remote"
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
    SpaceMembersChanged(SpaceMembersChangedEvent),
    SpaceAccessStateChanged(SpaceAccessStateChangedEvent),
    ClipboardNewContent(ClipboardNewContentEvent),
}

#[async_trait]
pub trait RealtimeTopicPort: Send + Sync {
    async fn subscribe(
        &self,
        consumer: &'static str,
        topics: &[RealtimeTopic],
    ) -> anyhow::Result<mpsc::Receiver<RealtimeEvent>>;
}
