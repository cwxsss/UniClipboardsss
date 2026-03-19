//! HTTP and WebSocket DTOs for the daemon transport layer.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uc_app::usecases::pairing::get_p2p_peers_snapshot::P2pPeerSnapshot;
use uc_core::network::PairedDevice;

use crate::state::DaemonPairingSessionSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub version: String,
    pub uptime_seconds: u64,
    pub workers: Vec<WorkerStatusDto>,
    pub connected_peers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStatusDto {
    pub name: String,
    pub health: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerSnapshotDto {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub addresses: Vec<String>,
    pub is_paired: bool,
    pub connected: bool,
    pub pairing_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceDto {
    pub peer_id: String,
    pub device_name: String,
    pub pairing_state: String,
    pub last_seen_at_ms: Option<i64>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingSessionSummaryDto {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
    pub state: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingSessionChangedPayload {
    pub session_id: String,
    pub state: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingVerificationPayload {
    pub session_id: String,
    pub peer_id: String,
    pub device_name: Option<String>,
    pub code: String,
    pub local_fingerprint: String,
    pub peer_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingFailurePayload {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerChangedPayload {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub addresses: Vec<String>,
    pub discovered: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerNameUpdatedPayload {
    pub peer_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerConnectionChangedPayload {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevicesChangedPayload {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWsSubscribeRequest {
    pub action: String,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWsEvent {
    pub topic: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub session_id: Option<String>,
    pub ts: i64,
    pub payload: Value,
}

impl From<P2pPeerSnapshot> for PeerSnapshotDto {
    fn from(value: P2pPeerSnapshot) -> Self {
        Self {
            peer_id: value.peer_id,
            device_name: value.device_name,
            addresses: value.addresses,
            is_paired: value.is_paired,
            connected: value.is_connected,
            pairing_state: value.pairing_state,
        }
    }
}

impl From<PairedDevice> for PairedDeviceDto {
    fn from(value: PairedDevice) -> Self {
        Self {
            peer_id: value.peer_id.to_string(),
            device_name: value.device_name,
            pairing_state: format!("{:?}", value.pairing_state),
            last_seen_at_ms: value
                .last_seen_at
                .map(|timestamp| timestamp.timestamp_millis()),
            connected: false,
        }
    }
}

impl From<DaemonPairingSessionSnapshot> for PairingSessionSummaryDto {
    fn from(value: DaemonPairingSessionSnapshot) -> Self {
        Self {
            session_id: value.session_id,
            peer_id: value.peer_id,
            device_name: value.device_name,
            state: value.state,
            updated_at_ms: value.updated_at_ms,
        }
    }
}
